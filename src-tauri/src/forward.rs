use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::error::{Error, Result};
use crate::models::{now_ms, Host};
use crate::ssh::connect_and_auth_via;

/// 一条正在运行的本地端口转发（本机 listen → 经 SSH 连到目标）
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ActiveForward {
    pub id: String,
    pub host_id: String,
    pub bind_host: String,
    pub bind_port: u16,
    pub dest_host: String,
    pub dest_port: u16,
    pub started_at: i64,
    pub connections: u64,
}

struct ForwardEntry {
    info: ActiveForward,
    conns: Arc<AtomicU64>,
    task: JoinHandle<()>,
}

#[derive(Default, Clone)]
pub struct ForwardManager {
    items: Arc<Mutex<HashMap<String, ForwardEntry>>>,
}

impl ForwardManager {
    /// 启动一条本地转发（-L）：本机 bind_host:bind_port → 远程 dest_host:dest_port
    pub async fn start(
        &self,
        host: &Host,
        jump: Option<Host>,
        bind_host: &str,
        bind_port: u16,
        dest_host: &str,
        dest_port: u16,
    ) -> Result<ActiveForward> {
        let dest_host = dest_host.trim().to_string();
        if dest_host.is_empty() || dest_port == 0 {
            return Err(Error::msg("目标地址/端口不能为空"));
        }
        let bind_host = if bind_host.trim().is_empty() {
            "127.0.0.1".to_string()
        } else {
            bind_host.trim().to_string()
        };

        // 先占端口，避免“端口占不上却先建了 SSH 连接”
        let listener = TcpListener::bind((bind_host.as_str(), bind_port))
            .await
            .map_err(|e| Error::msg(format!("监听 {bind_host}:{bind_port} 失败：{e}")))?;
        let actual_bind_port = listener
            .local_addr()
            .map(|a| a.port())
            .unwrap_or(bind_port);

        // 端口转发用独立连接，避免和终端会话互相影响
        let outcome = connect_and_auth_via(host, jump.as_ref()).await?;
        let handle = outcome.handle;
        let jump_handle = outcome.jump;

        let id = uuid::Uuid::new_v4().to_string();
        let conns = Arc::new(AtomicU64::new(0));
        let conns_task = conns.clone();
        let dest_for_task = dest_host.clone();

        let task = tokio::spawn(async move {
            // 跳板机连接要一直活着
            let _keep_jump = jump_handle;
            loop {
                let (socket, _peer) = match listener.accept().await {
                    Ok(v) => v,
                    Err(_) => break,
                };
                let channel = match handle
                    .channel_open_direct_tcpip(
                        dest_for_task.clone(),
                        dest_port as u32,
                        "127.0.0.1",
                        0,
                    )
                    .await
                {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                conns_task.fetch_add(1, Ordering::Relaxed);
                let mut stream = channel.into_stream();
                tokio::spawn(async move {
                    let mut socket = socket;
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
                });
            }
        });

        let info = ActiveForward {
            id: id.clone(),
            host_id: host.id.clone(),
            bind_host,
            bind_port: actual_bind_port,
            dest_host,
            dest_port,
            started_at: now_ms(),
            connections: 0,
        };
        self.items.lock()?.insert(
            id,
            ForwardEntry {
                info: info.clone(),
                conns,
                task,
            },
        );
        Ok(info)
    }

    pub fn list(&self) -> Vec<ActiveForward> {
        let Ok(guard) = self.items.lock() else {
            return Vec::new();
        };
        let mut out: Vec<ActiveForward> = guard
            .values()
            .map(|e| ActiveForward {
                connections: e.conns.load(Ordering::Relaxed),
                ..e.info.clone()
            })
            .collect();
        out.sort_by_key(|f| f.started_at);
        out
    }

    pub fn stop(&self, id: &str) -> Result<()> {
        let entry = self.items.lock()?.remove(id);
        if let Some(e) = entry {
            e.task.abort();
        }
        Ok(())
    }

    /// 关闭某个主机相关的所有转发
    pub fn stop_host(&self, host_id: &str) {
        if let Ok(mut guard) = self.items.lock() {
            let ids: Vec<String> = guard
                .iter()
                .filter(|(_, e)| e.info.host_id == host_id)
                .map(|(k, _)| k.clone())
                .collect();
            for id in ids {
                if let Some(e) = guard.remove(&id) {
                    e.task.abort();
                }
            }
        }
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.items.lock() {
            for (_, e) in guard.drain() {
                e.task.abort();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_target() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mgr = ForwardManager::default();
        let host = Host::default();
        let err = rt
            .block_on(mgr.start(&host, None, "127.0.0.1", 0, "", 0))
            .unwrap_err();
        assert!(err.to_string().contains("目标地址"));
        assert!(mgr.list().is_empty());
    }

    #[test]
    fn stop_unknown_is_ok() {
        let mgr = ForwardManager::default();
        assert!(mgr.stop("nope").is_ok());
        mgr.stop_host("nope");
        mgr.clear();
        assert!(mgr.list().is_empty());
    }
}

#[cfg(test)]
mod e2e {
    use super::*;
    use crate::models::{AuthConfig, AUTH_PASSWORD};
    use tokio::io::AsyncReadExt;

    /// 本地端口转发：连本机监听端口，流量应经 SSH 隧道到达「内网」服务。
    ///   TOMATO_TEST_SSH=127.0.0.1:2222 TOMATO_TEST_SSH_TARGET_PORT=2223
    #[test]
    fn local_forward_reaches_inner_service() {
        let Ok(addr) = std::env::var("TOMATO_TEST_SSH") else {
            eprintln!("跳过端口转发测试：未设置 TOMATO_TEST_SSH=127.0.0.1:2222");
            return;
        };
        let port: u16 = addr.rsplit(':').next().unwrap().parse().expect("端口");
        let target_port: u16 = std::env::var("TOMATO_TEST_SSH_TARGET_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2223);

        let host = Host {
            id: "sshd".into(),
            label: "sshd".into(),
            host: "127.0.0.1".into(),
            port,
            username: "root".into(),
            auth: AuthConfig {
                kind: AUTH_PASSWORD.into(),
                password: Some("tomato123".into()),
                ..Default::default()
            },
            ..Default::default()
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let mgr = ForwardManager::default();
            let fwd = mgr
                .start(&host, None, "127.0.0.1", 0, "127.0.0.1", target_port)
                .await
                .expect("启动本地转发");
            assert!(fwd.bind_port > 0, "应由系统分配监听端口");
            assert_eq!(mgr.list().len(), 1);

            // 连本机端口，应读到「内网」sshd 的版本横幅
            let mut sock = tokio::net::TcpStream::connect(("127.0.0.1", fwd.bind_port))
                .await
                .expect("连接本地监听端口");
            let mut buf = vec![0u8; 64];
            let n = tokio::time::timeout(
                std::time::Duration::from_secs(8),
                sock.read(&mut buf),
            )
            .await
            .expect("读取超时")
            .expect("读取失败");
            let text = String::from_utf8_lossy(&buf[..n]);
            assert!(text.contains("SSH-2.0"), "应透过隧道收到 SSH 横幅，实际：{text}");

            mgr.stop(&fwd.id).expect("停止转发");
            assert!(mgr.list().is_empty());
        });
    }
}