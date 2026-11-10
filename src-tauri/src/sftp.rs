use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use russh::client::Handle;
use russh_sftp::client::SftpSession;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::{Error, IntoAppError, Result};
use crate::models::{FileEntry, Host, TransferProgress};
use crate::ssh::{connect_and_auth, ClientHandler};

pub struct SftpEntry {
    pub session: Arc<SftpSession>,
    /// 保活：drop 掉 handle 会关闭连接
    pub handle: Handle<ClientHandler>,
}

/// 打开 SFTP 后的返回信息
pub struct SftpOpened {
    pub home: String,
    pub fingerprint: Option<String>,
}

#[derive(Default)]
pub struct SftpManager {
    sessions: Mutex<HashMap<String, SftpEntry>>,
}

fn emit_progress(app: &AppHandle, p: &TransferProgress) {
    let _ = app.emit("transfer-progress", p.clone());
}

fn entry_from_dir(entry: russh_sftp::client::fs::DirEntry) -> FileEntry {
    let meta = entry.metadata();
    let ftype = meta.file_type();
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    FileEntry {
        name: entry.file_name(),
        path: entry.path(),
        is_dir: ftype.is_dir(),
        is_symlink: ftype.is_symlink(),
        size: meta.len(),
        modified,
        mode: 0,
    }
}

impl SftpManager {
    pub async fn open(&self, id: &str, host: &Host) -> Result<SftpOpened> {
        let existing = {
            let guard = self.sessions.lock()?;
            guard.get(id).map(|e| e.session.clone())
        };
        if let Some(session) = existing {
            let home = session
                .canonicalize(".")
                .await
                .unwrap_or_else(|_| "/".to_string());
            return Ok(SftpOpened {
                home,
                fingerprint: None,
            });
        }

        let outcome = connect_and_auth(host).await?;
        let fingerprint = outcome.fingerprint.clone();
        let handle = outcome.handle;
        let channel = handle.channel_open_session().await.app_err()?;
        channel.request_subsystem(true, "sftp").await.app_err()?;
        let session = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| Error::msg(format!("SFTP 子系统启动失败：{e}")))?;
        let home = session
            .canonicalize(".")
            .await
            .unwrap_or_else(|_| "/".to_string());

        self.sessions.lock()?.insert(
            id.to_string(),
            SftpEntry {
                session: Arc::new(session),
                handle,
            },
        );
        Ok(SftpOpened { home, fingerprint })
    }

    pub async fn home(&self, id: &str) -> Result<String> {
        let session = self.session(id)?;
        Ok(session
            .canonicalize(".")
            .await
            .unwrap_or_else(|_| "/".to_string()))
    }

    fn session(&self, id: &str) -> Result<Arc<SftpSession>> {
        self.sessions
            .lock()?
            .get(id)
            .map(|e| e.session.clone())
            .ok_or_else(|| Error::msg("SFTP 会话不存在，请重新打开文件面板"))
    }

    pub fn close(&self, id: &str) {
        if let Ok(mut guard) = self.sessions.lock() {
            guard.remove(id);
        }
    }

    pub async fn list(&self, id: &str, path: &str) -> Result<Vec<FileEntry>> {
        let session = self.session(id)?;
        let path = normalize_remote(path);
        let mut items: Vec<FileEntry> = session
            .read_dir(path.as_str())
            .await
            .map_err(|e| Error::msg(format!("读取远程目录失败：{e}")))?
            .map(entry_from_dir)
            .filter(|e| e.name != "." && e.name != "..")
            .collect();
        items.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(items)
    }

    pub async fn mkdir(&self, id: &str, path: &str) -> Result<()> {
        let session = self.session(id)?;
        session
            .create_dir(path)
            .await
            .map_err(|e| Error::msg(format!("创建远程目录失败：{e}")))
    }

    pub async fn remove(&self, id: &str, path: &str, is_dir: bool) -> Result<()> {
        let session = self.session(id)?;
        let res = if is_dir {
            session.remove_dir(path).await
        } else {
            session.remove_file(path).await
        };
        res.map_err(|e| Error::msg(format!("删除远程文件失败：{e}")))
    }

    pub async fn rename(&self, id: &str, from: &str, to: &str) -> Result<()> {
        let session = self.session(id)?;
        session
            .rename(from, to)
            .await
            .map_err(|e| Error::msg(format!("重命名失败：{e}")))
    }

    /// 本地上传
    pub async fn upload(
        &self,
        app: &AppHandle,
        id: &str,
        local_path: &str,
        remote_path: &str,
    ) -> Result<()> {
        let session = self.session(id)?;
        let mut local = tokio::fs::File::open(local_path)
            .await
            .map_err(|e| Error::msg(format!("打开本地文件失败：{e}")))?;
        let total = local.metadata().await.map(|m| m.len()).unwrap_or(0);
        let mut remote = session
            .create(remote_path)
            .await
            .map_err(|e| Error::msg(format!("创建远程文件失败：{e}")))?;

        let tid = uuid::Uuid::new_v4().to_string();
        let name = basename(remote_path, '/');
        let mut progress = TransferProgress {
            id: tid.clone(),
            file_name: name,
            direction: "upload".into(),
            done: 0,
            total,
            status: "running".into(),
            message: None,
        };
        emit_progress(app, &progress);

        let mut buf = vec![0u8; 64 * 1024];
        let mut last = Instant::now();
        let result = loop {
            let n = match local.read(&mut buf).await {
                Ok(0) => break Ok(()),
                Ok(n) => n,
                Err(e) => break Err(Error::msg(format!("读取本地文件失败：{e}"))),
            };
            if let Err(e) = remote.write_all(&buf[..n]).await {
                break Err(Error::msg(format!("写入远程文件失败：{e}")));
            }
            progress.done += n as u64;
            if last.elapsed() > Duration::from_millis(120) {
                last = Instant::now();
                emit_progress(app, &progress);
            }
        };

        match result {
            Ok(()) => {
                let _ = remote.shutdown().await;
                progress.status = "done".into();
                emit_progress(app, &progress);
                Ok(())
            }
            Err(e) => {
                progress.status = "error".into();
                progress.message = Some(e.to_string());
                emit_progress(app, &progress);
                Err(e)
            }
        }
    }

    /// 远程下载
    pub async fn download(
        &self,
        app: &AppHandle,
        id: &str,
        remote_path: &str,
        local_path: &str,
    ) -> Result<()> {
        let session = self.session(id)?;
        let total = session
            .metadata(remote_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let mut remote = session
            .open(remote_path)
            .await
            .map_err(|e| Error::msg(format!("打开远程文件失败：{e}")))?;

        if let Some(parent) = std::path::Path::new(local_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut local = tokio::fs::File::create(local_path)
            .await
            .map_err(|e| Error::msg(format!("创建本地文件失败：{e}")))?;

        let mut progress = TransferProgress {
            id: uuid::Uuid::new_v4().to_string(),
            file_name: basename(remote_path, '/'),
            direction: "download".into(),
            done: 0,
            total,
            status: "running".into(),
            message: None,
        };
        emit_progress(app, &progress);

        let mut buf = vec![0u8; 64 * 1024];
        let mut last = Instant::now();
        let result = loop {
            let n = match remote.read(&mut buf).await {
                Ok(0) => break Ok(()),
                Ok(n) => n,
                Err(e) => break Err(Error::msg(format!("读取远程文件失败：{e}"))),
            };
            if let Err(e) = local.write_all(&buf[..n]).await {
                break Err(Error::msg(format!("写入本地文件失败：{e}")));
            }
            progress.done += n as u64;
            if last.elapsed() > Duration::from_millis(120) {
                last = Instant::now();
                emit_progress(app, &progress);
            }
        };

        match result {
            Ok(()) => {
                let _ = local.flush().await;
                progress.status = "done".into();
                emit_progress(app, &progress);
                Ok(())
            }
            Err(e) => {
                progress.status = "error".into();
                progress.message = Some(e.to_string());
                emit_progress(app, &progress);
                Err(e)
            }
        }
    }

    /// 两台远程主机之间直接对流传输（不落地到本地磁盘）
    pub async fn copy_remote(
        &self,
        app: &AppHandle,
        src_id: &str,
        src_path: &str,
        dst_id: &str,
        dst_path: &str,
    ) -> Result<()> {
        let src = self.session(src_id)?;
        let dst = self.session(dst_id)?;
        let total = src.metadata(src_path).await.map(|m| m.len()).unwrap_or(0);
        let mut reader = src
            .open(src_path)
            .await
            .map_err(|e| Error::msg(format!("打开源文件失败：{e}")))?;
        let mut writer = dst
            .create(dst_path)
            .await
            .map_err(|e| Error::msg(format!("创建目标文件失败：{e}")))?;

        let mut progress = TransferProgress {
            id: uuid::Uuid::new_v4().to_string(),
            file_name: basename(src_path, '/'),
            direction: "remoteToRemote".into(),
            done: 0,
            total,
            status: "running".into(),
            message: None,
        };
        emit_progress(app, &progress);

        let mut buf = vec![0u8; 64 * 1024];
        let mut last = Instant::now();
        let result = loop {
            let n = match reader.read(&mut buf).await {
                Ok(0) => break Ok(()),
                Ok(n) => n,
                Err(e) => break Err(Error::msg(format!("读取源文件失败：{e}"))),
            };
            if let Err(e) = writer.write_all(&buf[..n]).await {
                break Err(Error::msg(format!("写入目标文件失败：{e}")));
            }
            progress.done += n as u64;
            if last.elapsed() > Duration::from_millis(120) {
                last = Instant::now();
                emit_progress(app, &progress);
            }
        };

        match result {
            Ok(()) => {
                let _ = writer.shutdown().await;
                progress.status = "done".into();
                emit_progress(app, &progress);
                Ok(())
            }
            Err(e) => {
                progress.status = "error".into();
                progress.message = Some(e.to_string());
                emit_progress(app, &progress);
                Err(e)
            }
        }
    }
}

pub fn normalize_remote(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return "/".to_string();
    }
    trimmed.to_string()
}

pub fn basename(path: &str, sep: char) -> String {
    let s = path.trim_end_matches(sep);
    match s.rfind(sep) {
        Some(idx) if idx + 1 < s.len() => s[idx + 1..].to_string(),
        _ => s.to_string(),
    }
}
