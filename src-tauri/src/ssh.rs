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
            let path = host
                .auth
                .private_key_path
                .clone()
                .ok_or_else(|| Error::msg("该连接使用密钥认证，但未选择私钥文件"))?;
            let key = russh::keys::load_secret_key(&path, host.auth.passphrase.as_deref())
                .map_err(|e| Error::msg(format!("读取私钥失败（{path}）：{e}")))?;
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