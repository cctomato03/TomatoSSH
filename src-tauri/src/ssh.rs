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
}

#[derive(Default, Clone)]
pub struct SshManager {
    sessions: Arc<Mutex<HashMap<String, TermSession>>>,
}

pub struct ConnectOutcome {
    pub handle: Handle<ClientHandler>,
    pub fingerprint: Option<String>,
}

/// 打开终端后的返回信息
pub struct TerminalOpened {
    pub banner: String,
    pub fingerprint: Option<String>,
}

/// 建立连接并完成认证
pub async fn connect_and_auth(host: &Host) -> Result<ConnectOutcome> {
    let seen = Arc::new(Mutex::new(None::<String>));
    let handler = ClientHandler {
        expected: host.host_key_fingerprint.clone(),
        seen: seen.clone(),
    };

    let config = client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(Duration::from_secs(host.keepalive_secs.max(5))),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    };

    let addr = (host.host.as_str(), host.port);
    let mut handle = match client::connect(Arc::new(config), addr, handler).await {
        Ok(h) => h,
        Err(e) => {
            let seen_fp = seen.lock().unwrap().clone();
            if let (Some(expected), Some(actual)) = (&host.host_key_fingerprint, &seen_fp) {
                if expected != actual {
                    return Err(Error::msg(format!(
                        "主机密钥指纹不匹配，连接已中止。\n期望：{expected}\n实际：{actual}\n\n若确认服务器密钥已变更（例如重装系统），请编辑该连接并清除已保存的指纹。"
                    )));
                }
            }
            return Err(Error::msg(format!("无法连接到 {}:{} —— {e}", host.host, host.port)));
        }
    };

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

    let fingerprint = seen.lock().unwrap().clone();
    Ok(ConnectOutcome {
        handle,
        fingerprint,
    })
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
    ) -> Result<TerminalOpened> {
        let outcome = connect_and_auth(&host).await?;
        let fingerprint = outcome.fingerprint.clone();
        let handle = outcome.handle;

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
