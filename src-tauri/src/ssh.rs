use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::anyhow;
use russh::client::{self, Handle, Msg};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{ChannelMsg, ChannelWriteHalf, Disconnect};
use tauri::{AppHandle, Emitter};

use crate::crypto::b64_encode;
use crate::error::{Error, IntoAppError, Result};
use crate::models::{Host, AUTH_KEY, AUTH_PASSWORD};

pub struct ClientHandler {
    /// 期望的主机公钥指纹（TOFU 记录）
    expected: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
}

impl client::Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> std::result::Result<bool, Self::Error> {
        let fp = server_public_key
            .public_key()
            .fingerprint(HashAlg::Sha256)
            .to_string();
        *self.seen.lock().unwrap() = Some(fp.clone());
        Ok(match &self.expected {
            Some(exp) => exp == &fp,
            None => true,
        })
    }
}

pub struct TermSession {
    pub write: Arc<ChannelWriteHalf<Msg>>,
    pub handle: Handle<ClientHandler>,
    /// 走跳板机时保留跳板机连接，drop 会断掉整条链路
    #[allow(dead_code)]
    pub jump: Option<Handle<ClientHandler>>,
}

#[derive(Default, Clone)]
pub struct SshManager {
    sessions: Arc<Mutex<HashMap<String, TermSession>>>,
}

pub struct ConnectOutcome {
    pub handle: Handle<ClientHandler>,
    pub fingerprint: Option<String>,
    /// 若走了跳板机，这里保留跳板机的连接（drop 会断开整条链路）
    pub jump: Option<Handle<ClientHandler>>,
}

/// 打开终端后的返回信息
pub struct TerminalOpened {
    pub banner: String,
    pub fingerprint: Option<String>,
}

fn client_config(host: &Host) -> Arc<client::Config> {
    Arc::new(client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(Duration::from_secs(host.keepalive_secs.max(5))),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    })
}

/// TCP 直连目标（不做认证）
async fn tcp_connect(
    host: &Host,
) -> Result<(Handle<ClientHandler>, Arc<Mutex<Option<String>>>)> {
    let seen = Arc::new(Mutex::new(None::<String>));
    let handler = ClientHandler {
        expected: host.host_key_fingerprint.clone(),
        seen: seen.clone(),
    };
    let addr = (host.host.as_str(), host.port);
    match client::connect(client_config(host), addr, handler).await {
        Ok(h) => Ok((h, seen)),
        Err(e) => {
            let seen_fp = seen.lock().unwrap().clone();
            if let (Some(expected), Some(actual)) = (&host.host_key_fingerprint, &seen_fp) {
                if expected != actual {
                    return Err(Error::msg(format!(
                        "主机密钥指纹不匹配，连接已中止。\n期望：{expected}\n实际：{actual}\n\n若确认服务器密钥已变更（例如重装系统），请编辑该连接并清除已保存的指纹。"
                    )));
                }
            }
            Err(Error::msg(format!(
                "无法连接到 {}:{} —— {e}",
                host.host, host.port
            )))
        }
    }
}

/// 完成认证
async fn authenticate(handle: &mut Handle<ClientHandler>, host: &Host) -> Result<()> {
    match host.auth.kind.as_str() {
        AUTH_PASSWORD => {
            let password = host.auth.password.clone().unwrap_or_default();
            if password.is_empty() {
                return Err(Error::msg("该连接使用密码认证，但密码为空"));
            }
            let res = handle
                .authenticate_password(host.username.clone(), password)
                .await
                .app_err()?;
            if !res.success() {
                return Err(Error::msg("密码认证失败，请检查用户名与密码"));
            }
        }
        AUTH_KEY => {
            // 优先用保险库里的私钥内容（可云同步、跨机器可用），其次才用本机路径
            let pem = host
                .auth
                .private_key_pem
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let key = match pem {
                Some(pem) => {
                    let label = host.auth.private_key_name.as_deref().unwrap_or("保险库私钥");
                    russh::keys::decode_secret_key(pem, host.auth.passphrase.as_deref())
                        .map_err(|e| Error::msg(format!("解析私钥失败（{label}）：{e}")))?
                }
                None => {
                    let path = host.auth.private_key_path.clone().ok_or_else(|| {
                        Error::msg("该连接使用密钥认证，但没有导入私钥（也没填本机路径）")
                    })?;
                    russh::keys::load_secret_key(&path, host.auth.passphrase.as_deref())
                        .map_err(|e| Error::msg(format!("读取私钥失败（{path}）：{e}")))?
                }
            };
            let hash = handle.best_supported_rsa_hash().await.app_err()?.flatten();
            let res = handle
                .authenticate_publickey(
                    host.username.clone(),
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                )
                .await
                .app_err()?;
            if !res.success() {
                return Err(Error::msg("密钥认证失败，请确认公钥已加入服务器 authorized_keys"));
            }
        }
        other => {
            return Err(Error::msg(format!(
                "暂不支持的认证方式：{other}（当前支持密码与私钥）"
            )));
        }
    }
    Ok(())
}

/// 导入进保险库所需的信息
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ImportedKey {
    /// 文件名（仅用于显示）
    pub name: String,
    /// 私钥内容本身
    pub content: String,
    /// ED25519 / RSA / ...
    pub key_type: String,
    /// SHA256:... 指纹
    pub fingerprint: String,
    /// 私钥是否带口令加密
    pub encrypted: bool,
}

/// 私钥文件大小上限
pub const MAX_KEY_BYTES: u64 = 512 * 1024;

/// 判断私钥内容本身是否被口令加密
///   - OpenSSH 新格式：base64 主体解出来后前 15 字节是 "openssh-key-v1\0"，
///     紧跟 4 字节大端长度 + ciphername；ciphername != "none" 即加密
///   - 传统 PEM：出现 Proc-Type: 4,ENCRYPTED
fn key_content_is_encrypted(content: &str) -> bool {
    if content.contains("Proc-Type: 4,ENCRYPTED") || content.contains("ENCRYPTED-----") {
        return true;
    }
    let body: String = content
        .lines()
        .filter(|l| !l.trim_start().starts_with("-----"))
        .collect::<Vec<_>>()
        .join("");
    let Ok(raw) = crate::crypto::b64_decode(body.trim()) else {
        return false;
    };
    const MAGIC: &[u8] = b"openssh-key-v1\0";
    if raw.len() < MAGIC.len() + 4 || !raw.starts_with(MAGIC) {
        return false;
    }
    let rest = &raw[MAGIC.len()..];
    let n = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
    if n == 0 || n > 64 || rest.len() < 4 + n {
        return false;
    }
    let cipher = String::from_utf8_lossy(&rest[4..4 + n]).to_string();
    cipher != "none"
}
/// 读取并校验一个私钥文件，返回可存进保险库的内容
pub fn import_private_key_file(path: &str, passphrase: Option<&str>) -> Result<ImportedKey> {
    let meta = std::fs::metadata(path)
        .map_err(|e| Error::msg(format!("读取私钥文件失败（{path}）：{e}")))?;
    if meta.len() > MAX_KEY_BYTES {
        return Err(Error::msg(format!(
            "私钥文件过大（{} 字节），上限 {} 字节",
            meta.len(),
            MAX_KEY_BYTES
        )));
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| Error::msg(format!("读取私钥文件失败（{path}）：{e}")))?;
    if !content.contains("PRIVATE KEY-----") {
        return Err(Error::msg(
            "这不是一个私钥文件（缺少 PRIVATE KEY 头，注意别选成 .pub 公钥）",
        ));
    }
    let name = std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "private-key".to_string());
    let encrypted_hint = key_content_is_encrypted(&content);

    match russh::keys::decode_secret_key(&content, passphrase) {
        Ok(key) => {
            // is_rsa()/is_ed25519() 会消耗 Algorithm，先按引用判断再格式化
            let alg = key.algorithm();
            let alg_name = format!("{alg}");
            let key_type = if alg.clone().is_rsa() {
                "RSA".to_string()
            } else if alg.is_ed25519() {
                "ED25519".to_string()
            } else {
                alg_name
            };
            let fingerprint = key
                .fingerprint(russh::keys::HashAlg::Sha256)
                .to_string();
            Ok(ImportedKey {
                name,
                content,
                key_type,
                fingerprint,
                encrypted: encrypted_hint,
            })
        }
        // 带口令的私钥即便这次没能解开也允许导入，用户填上口令即可使用
        Err(e) => {
            let lower = e.to_string().to_ascii_lowercase();
            let looks_encrypted = encrypted_hint
                || lower.contains("encrypted")
                || lower.contains("passphrase")
                || lower.contains("password");
            if looks_encrypted {
                Ok(ImportedKey {
                    name,
                    content,
                    key_type: "加密私钥".to_string(),
                    fingerprint: String::new(),
                    encrypted: true,
                })
            } else {
                Err(Error::msg(format!("私钥解析失败：{e}")))
            }
        }
    }
}
/// 建立连接并完成认证（`jump` 存在时经跳板机转发）
pub async fn connect_and_auth_via(host: &Host, jump: Option<&Host>) -> Result<ConnectOutcome> {
    let (mut handle, seen, jump_keep) = match jump {
        None => {
            let (h, seen) = tcp_connect(host).await?;
            (h, seen, None)
        }
        Some(jump_host) => {
            // ① 先连并登录跳板机
            let (mut jump_handle, _jump_seen) = tcp_connect(jump_host).await?;
            if let Err(e) = authenticate(&mut jump_handle, jump_host).await {
                return Err(Error::msg(format!(
                    "跳板机（{}@{}）登录失败：{e}",
                    jump_host.username, jump_host.host
                )));
            }
            // ② 通过跳板机开一条到目标主机的 TCP 通道
            let channel = jump_handle
                .channel_open_direct_tcpip(
                    host.host.clone(),
                    host.port as u32,
                    "127.0.0.1",
                    0,
                )
                .await
                .map_err(|e| {
                    Error::msg(format!(
                        "经跳板机连接 {}:{} 失败：{e}",
                        host.host, host.port
                    ))
                })?;
            // ③ 在这条通道上跑 SSH 握手
            let seen = Arc::new(Mutex::new(None::<String>));
            let handler = ClientHandler {
                expected: host.host_key_fingerprint.clone(),
                seen: seen.clone(),
            };
            let handle = client::connect_stream(client_config(host), channel.into_stream(), handler)
                .await
                .map_err(|e| {
                    let seen_fp = seen.lock().unwrap().clone();
                    if let (Some(expected), Some(actual)) =
                        (&host.host_key_fingerprint, &seen_fp)
                    {
                        if expected != actual {
                            return Error::msg(format!(
                                "主机密钥指纹不匹配，连接已中止。\n期望：{expected}\n实际：{actual}"
                            ));
                        }
                    }
                    Error::msg(format!(
                        "无法连接到 {}:{} —— {e}",
                        host.host, host.port
                    ))
                })?;
            (handle, seen, Some(jump_handle))
        }
    };

    authenticate(&mut handle, host).await?;
    let fingerprint = seen.lock().unwrap().clone();
    Ok(ConnectOutcome {
        handle,
        fingerprint,
        jump: jump_keep,
    })
}

/// 建立连接并完成认证（不带跳板机）
pub async fn connect_and_auth(host: &Host) -> Result<ConnectOutcome> {
    connect_and_auth_via(host, None).await
}

impl SshManager {
    /// 打开一个交互式终端（PTY + shell），数据通过事件流回前端
    pub async fn open_terminal(
        &self,
        app: AppHandle,
        session_id: String,
        host: Host,
        cols: u32,
        rows: u32,
        jump: Option<Host>,
    ) -> Result<TerminalOpened> {
        let outcome = connect_and_auth_via(&host, jump.as_ref()).await?;
        let fingerprint = outcome.fingerprint.clone();
        let handle = outcome.handle;
        let jump_handle = outcome.jump;

        let channel = handle.channel_open_session().await.app_err()?;
        channel
            .request_pty(
                false,
                "xterm-256color",
                cols.max(20),
                rows.max(5),
                0,
                0,
                &[],
            )
            .await
            .app_err()?;
        channel.request_shell(true).await.app_err()?;

        let (mut read_half, write_half) = channel.split();
        let write = Arc::new(write_half);

        self.sessions.lock().unwrap().insert(
            session_id.clone(),
            TermSession {
                write: write.clone(),
                handle,
                jump: jump_handle,
            },
        );

        let app_data = app.clone();
        let sid = session_id.clone();
        let app_exit = app.clone();
        let sessions = self.sessions.clone();
        tokio::spawn(async move {
            let mut exit_code: Option<u32> = None;
            loop {
                match read_half.wait().await {
                    Some(ChannelMsg::Data { data }) => {
                        let _ = app_data.emit(&format!("term-data:{sid}"), b64_encode(&data));
                    }
                    Some(ChannelMsg::ExtendedData { data, ext }) => {
                        if ext == 1 {
                            let _ = app_data.emit(&format!("term-data:{sid}"), b64_encode(&data));
                        }
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        exit_code = Some(exit_status);
                    }
                    Some(ChannelMsg::ExitSignal { .. }) => {}
                    Some(ChannelMsg::Close) | None => break,
                    Some(ChannelMsg::Eof) => {}
                    _ => {}
                }
            }
            // 会话结束后释放连接（drop Handle 会真正断开 TCP）
            if let Ok(mut guard) = sessions.lock() {
                guard.remove(&sid);
            }
            let _ = app_exit.emit(
                &format!("term-exit:{sid}"),
                serde_json::json!({ "code": exit_code, "message": null }),
            );
        });

        let banner = format!(
            "\x1b[38;5;244m—— 已连接 {}@{}:{} ——\x1b[0m",
            host.username, host.host, host.port
        );
        Ok(TerminalOpened {
            banner,
            fingerprint,
        })
    }

    pub async fn write(&self, session_id: &str, data: &[u8]) -> Result<()> {
        let write = {
            let guard = self.sessions.lock()?;
            guard.get(session_id).map(|s| s.write.clone())
        };
        let write = write.ok_or_else(|| Error::msg("会话不存在或已关闭"))?;
        write.data_bytes(data.to_vec()).await.app_err()?;
        Ok(())
    }

    pub async fn resize(&self, session_id: &str, cols: u32, rows: u32) -> Result<()> {
        let write = {
            let guard = self.sessions.lock()?;
            guard.get(session_id).map(|s| s.write.clone())
        };
        if let Some(write) = write {
            write
                .window_change(cols.max(20), rows.max(5), 0, 0)
                .await
                .app_err()?;
        }
        Ok(())
    }

    pub async fn close(&self, session_id: &str) -> Result<()> {
        let session = {
            let mut guard = self.sessions.lock()?;
            guard.remove(session_id)
        };
        if let Some(session) = session {
            let _ = session.write.eof().await;
            let _ = session.write.close().await;
            let _ = session
                .handle
                .disconnect(Disconnect::ByApplication, "user closed", "zh")
                .await;
        }
        Ok(())
    }

    pub fn has_session(&self, session_id: &str) -> bool {
        self.sessions
            .lock()
            .map(|g| g.contains_key(session_id))
            .unwrap_or(false)
    }
}

/// 便于在错误处理时打印中文提示的小工具
pub fn describe(e: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("{e}")
}

#[cfg(test)]
mod e2e {
    use super::*;
    use crate::models::{AuthConfig, AUTH_PASSWORD};

    fn host(id: &str, port: u16) -> Host {
        Host {
            id: id.into(),
            label: id.into(),
            host: "127.0.0.1".into(),
            port,
            username: "root".into(),
            auth: AuthConfig {
                kind: AUTH_PASSWORD.into(),
                password: Some("tomato123".into()),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// ProxyJump：先登跳板机，再通过它连「只有内网能访问」的目标并执行命令。
    ///   容器准备：一个 sshd 监听 22（映射到宿主 2222），另一个 sshd 只监听容器内 2223。
    ///   TOMATO_TEST_SSH=127.0.0.1:2222 TOMATO_TEST_SSH_TARGET_PORT=2223
    #[test]
    fn connects_through_jump_host() {
        let Ok(addr) = std::env::var("TOMATO_TEST_SSH") else {
            eprintln!("跳过 ProxyJump 测试：未设置 TOMATO_TEST_SSH=127.0.0.1:2222");
            return;
        };
        let port: u16 = addr.rsplit(':').next().unwrap().parse().expect("端口");
        let target_port: u16 = std::env::var("TOMATO_TEST_SSH_TARGET_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2223);

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let jump = host("jump", port);
            let target = host("target", target_port);

            // 直连跳板机
            let direct = connect_and_auth(&jump).await.expect("直连跳板机");
            assert!(direct.jump.is_none());

            // 目标端口在宿主上并不开放，必须经跳板机才能连上
            let direct_target = connect_and_auth(&target).await;
            assert!(
                direct_target.is_err(),
                "内网目标不应能从本机直连（否则这个测试没有意义）"
            );

            // 经跳板机连接
            let outcome = connect_and_auth_via(&target, Some(&jump))
                .await
                .expect("经跳板机连接失败");
            assert!(outcome.jump.is_some(), "应保留跳板机连接");

            // 在隧道上真的执行一条命令
            let mut channel = outcome
                .handle
                .channel_open_session()
                .await
                .expect("打开会话通道");
            channel.exec(true, "echo TOMATO_JUMP_OK").await.expect("执行命令");
            let mut out = Vec::new();
            while let Some(msg) = channel.wait().await {
                match msg {
                    ChannelMsg::Data { data } => out.extend_from_slice(&data),
                    ChannelMsg::Close | ChannelMsg::Eof => break,
                    _ => {}
                }
            }
            let text = String::from_utf8_lossy(&out);
            assert!(
                text.contains("TOMATO_JUMP_OK"),
                "命令应经隧道返回，实际：{text}"
            );
        });
    }
}
#[cfg(test)]
mod key_tests {
    use super::*;

    /// 带口令的 ed25519 私钥（口令：blabla），来自 russh 自带测试数据
    const ENCRYPTED_ED25519: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jYmMAAAAGYmNyeXB0AAAAGAAAABDLGyfA39
J2FcJygtYqi5ISAAAAEAAAAAEAAAAzAAAAC3NzaC1lZDI1NTE5AAAAIN+Wjn4+4Fcvl2Jl
KpggT+wCRxpSvtqqpVrQrKN1/A22AAAAkOHDLnYZvYS6H9Q3S3Nk4ri3R2jAZlQlBbUos5
FkHpYgNw65KCWCTXtP7ye2czMC3zjn2r98pJLobsLYQgRiHIv/CUdAdsqbvMPECB+wl/UQ
e+JpiSq66Z6GIt0801skPh20jxOO3F52SoX1IeO5D5PXfZrfSZlw6S8c7bwyp2FHxDewRx
7/wNsnDM0T7nLv/Q==
-----END OPENSSH PRIVATE KEY-----";

    fn temp_key_file(content: &str, name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("tskey-{}-{name}", uuid::Uuid::new_v4()));
        std::fs::write(&path, content).expect("写入临时私钥");
        path
    }

    #[test]
    fn imports_encrypted_key_with_passphrase() {
        let path = temp_key_file(ENCRYPTED_ED25519, "id_ed25519");
        let imported = import_private_key_file(path.to_str().unwrap(), Some("blabla"))
            .expect("带口令应能导入");
        assert_eq!(imported.key_type, "ED25519");
        assert!(imported.encrypted, "应识别为加密私钥");
        assert!(
            imported.fingerprint.starts_with("SHA256:"),
            "指纹：{}",
            imported.fingerprint
        );
        assert!(imported.content.contains("PRIVATE KEY-----"));
        assert_eq!(imported.name, path.file_name().unwrap().to_string_lossy());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn imports_encrypted_key_without_passphrase_but_flags_it() {
        let path = temp_key_file(ENCRYPTED_ED25519, "id_ed25519");
        let imported = import_private_key_file(path.to_str().unwrap(), None)
            .expect("加密私钥即使没口令也应允许导入");
        assert!(imported.encrypted);
        assert_eq!(imported.key_type, "加密私钥");
        assert!(imported.fingerprint.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rejects_public_key_file() {
        let path = temp_key_file(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample user@host",
            "id_ed25519.pub",
        );
        let err = import_private_key_file(path.to_str().unwrap(), None).unwrap_err();
        assert!(err.to_string().contains("私钥"), "{err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rejects_missing_file() {
        let err = import_private_key_file("Z:\\nope\\id_rsa", None).unwrap_err();
        assert!(err.to_string().contains("读取私钥文件失败"), "{err}");
    }

    #[test]
    fn vault_content_is_what_the_auth_path_decodes() {
        // 认证时优先取 private_key_pem，这里验证该内容能被正常解析（等价于 ssh 登录前的校验）
        assert!(russh::keys::decode_secret_key(ENCRYPTED_ED25519, Some("blabla")).is_ok());
        // 口令错误时必须失败
        assert!(russh::keys::decode_secret_key(ENCRYPTED_ED25519, Some("wrong")).is_err());
    }
}
#[cfg(test)]
mod key_e2e {
    use super::*;
    use crate::models::{AuthConfig, AUTH_KEY};

    /// 无口令的 RSA 私钥（russh 自带测试数据），这里当作「保险库里存的内容」使用
    const KEY_CONTENT: &str = r#"-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAABFwAAAAdzc2gtcn
NhAAAAAwEAAQAAAQEAuSvQ9m76zhRB4m0BUKPf17lwccj7KQ1Qtse63AOqP/VYItqEH8un
rxPogXNBgrcCEm/ccLZZsyE3qgp3DRQkkqvJhZ6O8VBPsXxjZesRCqoFNCczy+Mf0R/Qmv
Rnpu5+4DDLz0p7vrsRZW9ji/c98KzxeUonWgkplQaCBYLN875WdeUYMGtb1MLfNCEj177j
gZl3CzttLRK3su6dckowXcXYv1gPTPZAwJb49J43o1QhV7+1zdwXvuFM6zuYHdu9ZHSKir
6k1dXOET3/U+LWG5ofAo8oxUWv/7vs6h7MeajwkUeIBOWYtD+wGYRvVpxvj7nyOoWtg+jm
0X6ndnsD+QAAA8irV+ZAq1fmQAAAAAdzc2gtcnNhAAABAQC5K9D2bvrOFEHibQFQo9/XuX
BxyPspDVC2x7rcA6o/9Vgi2oQfy6evE+iBc0GCtwISb9xwtlmzITeqCncNFCSSq8mFno7x
UE+xfGNl6xEKqgU0JzPL4x/RH9Ca9Gem7n7gMMvPSnu+uxFlb2OL9z3wrPF5SidaCSmVBo
IFgs3zvlZ15Rgwa1vUwt80ISPXvuOBmXcLO20tErey7p1ySjBdxdi/WA9M9kDAlvj0njej
VCFXv7XN3Be+4UzrO5gd271kdIqKvqTV1c4RPf9T4tYbmh8CjyjFRa//u+zqHsx5qPCRR4
gE5Zi0P7AZhG9WnG+PufI6ha2D6ObRfqd2ewP5AAAAAwEAAQAAAQAdELqhI/RsSpO45eFR
9hcZtnrm8WQzImrr9dfn1w9vMKSf++rHTuFIQvi48Q10ZiOGH1bbvlPAIVOqdjAPtnyzJR
HhzmyjhjasJlk30zj+kod0kz63HzSMT9EfsYNfmYoCyMYFCKz52EU3xc87Vhi74XmZz0D0
CgIj6TyZftmzC4YJCiwwU8K+29nxBhcbFRxpgwAksFL6PCSQsPl4y7yvXGcX+7lpZD8547
v58q3jIkH1g2tBOusIuaiphDDStVJhVdKA55Z0Kju2kvCqsRIlf1efrq43blRgJFFFCxNZ
8Cpolt4lOHhg+o3ucjILlCOgjDV8dB21YLxmgN5q+xFNAAAAgQC1P+eLUkHDFXnleCEVrW
xL/DFxEyneLQz3IawGdw7cyAb7vxsYrGUvbVUFkxeiv397pDHLZ5U+t5cOYDBZ7G43Mt2g
YfWBuRNvYhHA9Sdf38m5qPA6XCvm51f+FxInwd/kwRKH01RHJuRGsl/4Apu4DqVob8y00V
WTYyV6JBNDkQAAAIEA322lj7ZJXfK/oLhMM/RS+DvaMea1g/q43mdRJFQQso4XRCL6IIVn
oZXFeOxrMIRByVZBw+FSeB6OayWcZMySpJQBo70GdJOc3pJb3js0T+P2XA9+/jwXS58K9a
+IkgLkv9XkfxNGNKyPEEzXC8QQzvjs1LbmO59VLko8ypwHq/cAAACBANQqaULI0qdwa0vm
d3Ae1+k3YLZ0kapSQGVIMT2lkrhKV35tj7HIFpUPa4vitHzcUwtjYhqFezVF+JyPbJ/Fsp
XmEc0g1fFnQp5/SkUwoN2zm8Up52GBelkq2Jk57mOMzWO0QzzNuNV/feJk02b2aE8rrAqP
QR+u0AypRPmzHnOPAAAAEXJvb3RAMTQwOTExNTQ5NDBkAQ==
-----END OPENSSH PRIVATE KEY-----"#;

    /// 只用「保险库里的私钥内容」登录，本机不存在任何私钥文件 ——
    /// 模拟换一台电脑后、路径没了但内容已随云同步过来的场景。
    ///   TOMATO_TEST_SSH=127.0.0.1:2222（容器里 root 的 authorized_keys 需包含该公钥）
    #[test]
    fn authenticates_with_private_key_content() {
        let Ok(addr) = std::env::var("TOMATO_TEST_SSH") else {
            eprintln!("跳过私钥内容测试：未设置 TOMATO_TEST_SSH");
            return;
        };
        let port: u16 = addr.rsplit(':').next().unwrap().parse().expect("端口");

        let host = Host {
            id: "kv".into(),
            label: "kv".into(),
            host: "127.0.0.1".into(),
            port,
            username: "root".into(),
            auth: AuthConfig {
                kind: AUTH_KEY.into(),
                // 关键：只有内容，路径为空
                private_key_pem: Some(KEY_CONTENT.to_string()),
                private_key_path: None,
                private_key_name: Some("vault-key".into()),
                ..Default::default()
            },
            ..Default::default()
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let outcome = connect_and_auth(&host).await.expect("用保险库私钥内容登录");
            let mut channel = outcome
                .handle
                .channel_open_session()
                .await
                .expect("打开会话通道");
            channel
                .exec(true, "echo TOMATO_KEY_CONTENT_OK")
                .await
                .expect("执行命令");
            let mut out = Vec::new();
            while let Some(msg) = channel.wait().await {
                match msg {
                    ChannelMsg::Data { data } => out.extend_from_slice(&data),
                    ChannelMsg::Close | ChannelMsg::Eof => break,
                    _ => {}
                }
            }
            let text = String::from_utf8_lossy(&out);
            assert!(
                text.contains("TOMATO_KEY_CONTENT_OK"),
                "命令应返回，实际：{text}"
            );
        });
    }
}