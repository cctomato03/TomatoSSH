use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use russh::client::Handle;
use russh_sftp::client::SftpSession;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::{Error, IntoAppError, Result};
use crate::models::{FileEntry, Host, TransferProgress};
use crate::ssh::{connect_and_auth, ClientHandler};

/// 传输分片大小
const CHUNK: usize = 64 * 1024;

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

/// 传输进度回调：生产环境由 tauri AppHandle 实现，测试里可以换成收集器
pub trait TransferSink: Send + Sync {
    fn progress(&self, p: &TransferProgress);
}

impl TransferSink for AppHandle {
    fn progress(&self, p: &TransferProgress) {
        let _ = self.emit("transfer-progress", p.clone());
    }
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

// ---------------------------------------------------------------- 路径小工具（递归传输用）

/// 远程路径拼接（SFTP 一律用 `/`）
fn join_remote(dir: &str, name: &str) -> String {
    if name.is_empty() {
        return dir.to_string();
    }
    if dir.is_empty() || dir == "/" {
        format!("/{name}")
    } else if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

fn parent_remote(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => trimmed[..i].to_string(),
        None => String::new(),
    }
}

fn basename_any(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    match trimmed.rfind(['/', '\\']) {
        Some(i) => trimmed[i + 1..].to_string(),
        None => trimmed.to_string(),
    }
}

/// 扫描本地目录，返回 (绝对路径, 相对路径[用 / 分隔], 大小) 的文件列表。
/// 跟随普通文件；遇到"指向目录的符号链接"直接跳过，避免循环。
fn scan_local_dir(dir: &str) -> Result<Vec<(String, String, u64)>> {
    let mut out = Vec::new();
    let mut stack: Vec<(std::path::PathBuf, String)> = vec![(std::path::PathBuf::from(dir), String::new())];
    while let Some((abs, rel)) = stack.pop() {
        let read = std::fs::read_dir(&abs)
            .map_err(|e| Error::msg(format!("读取本地文件夹失败（{}）：{e}", abs.display())))?;
        for entry in read {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let name = entry.file_name().to_string_lossy().to_string();
            let child_abs = entry.path();
            let child_rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            let link_meta = match std::fs::symlink_metadata(&child_abs) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if link_meta.is_dir() {
                stack.push((child_abs, child_rel));
                continue;
            }
            // 符号链接：如果指向目录就跳过（避免死循环），否则按文件处理
            if link_meta.file_type().is_symlink() {
                if let Ok(target) = std::fs::metadata(&child_abs) {
                    if target.is_dir() {
                        continue;
                    }
                }
            }
            let size = std::fs::metadata(&child_abs).map(|m| m.len()).unwrap_or(0);
            out.push((child_abs.to_string_lossy().to_string(), child_rel, size));
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(out)
}

/// 递归列出远程目录下的所有文件：(完整路径, 大小)
async fn scan_remote_dir(session: &SftpSession, dir: &str) -> Result<Vec<(String, u64)>> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_string()];
    while let Some(cur) = stack.pop() {
        let entries = session
            .read_dir(cur.as_str())
            .await
            .map_err(|e| Error::msg(format!("读取远程目录失败（{cur}）：{e}")))?;
        for entry in entries {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let meta = entry.metadata();
            let path = entry.path();
            if meta.file_type().is_dir() {
                stack.push(path);
            } else {
                files.push((path, meta.len()));
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

/// 确保远程目录存在（逐级创建，已存在视为成功）
async fn ensure_remote_dir(
    session: &SftpSession,
    dir: &str,
    created: &mut HashSet<String>,
) -> Result<()> {
    if dir.is_empty() || dir == "/" || created.contains(dir) {
        return Ok(());
    }
    let mut chain = Vec::new();
    let mut cur = dir.trim_end_matches('/').to_string();
    while !cur.is_empty() && cur != "/" {
        if created.contains(&cur) {
            break;
        }
        chain.push(cur.clone());
        cur = parent_remote(&cur);
    }
    for d in chain.into_iter().rev() {
        if !session.try_exists(d.as_str()).await.unwrap_or(false) {
            let _ = session.create_dir(d.as_str()).await;
            if !session.try_exists(d.as_str()).await.unwrap_or(false) {
                return Err(Error::msg(format!("创建远程目录失败：{d}")));
            }
        }
        created.insert(d);
    }
    Ok(())
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

    /// 删除远程文件或目录（目录会递归删除）
    pub async fn remove(&self, id: &str, path: &str, is_dir: bool) -> Result<()> {
        let session = self.session(id)?;
        if !is_dir {
            return session
                .remove_file(path)
                .await
                .map_err(|e| Error::msg(format!("删除远程文件失败：{e}")));
        }

        // 先收集整棵树，再自底向上删除
        let mut dirs: Vec<String> = Vec::new();
        let mut files: Vec<String> = Vec::new();
        let mut stack = vec![path.to_string()];
        while let Some(cur) = stack.pop() {
            dirs.push(cur.clone());
            let entries = match session.read_dir(cur.as_str()).await {
                Ok(e) => e,
                Err(e) => return Err(Error::msg(format!("读取远程目录失败（{cur}）：{e}"))),
            };
            for entry in entries {
                let name = entry.file_name();
                if name == "." || name == ".." {
                    continue;
                }
                let child = entry.path();
                if entry.metadata().file_type().is_dir() {
                    stack.push(child);
                } else {
                    files.push(child);
                }
            }
        }
        for f in files {
            session
                .remove_file(f.as_str())
                .await
                .map_err(|e| Error::msg(format!("删除远程文件失败（{f}）：{e}")))?;
        }
        dirs.sort_by_key(|d| std::cmp::Reverse(d.len()));
        for d in dirs {
            session
                .remove_dir(d.as_str())
                .await
                .map_err(|e| Error::msg(format!("删除远程目录失败（{d}）：{e}")))?;
        }
        Ok(())
    }

    pub async fn rename(&self, id: &str, from: &str, to: &str) -> Result<()> {
        let session = self.session(id)?;
        session
            .rename(from, to)
            .await
            .map_err(|e| Error::msg(format!("重命名失败：{e}")))
    }

    // ------------------------------------------------------------ 上传

    /// 上传本地文件或整个文件夹（`remote_path` 为目标路径本身）
    pub async fn upload(
        &self,
        sink: &dyn TransferSink,
        id: &str,
        local_path: &str,
        remote_path: &str,
    ) -> Result<()> {
        let session = self.session(id)?;
        let meta = std::fs::metadata(local_path)
            .map_err(|e| Error::msg(format!("无法访问本地路径（{local_path}）：{e}")))?;
        let name = basename_any(local_path);

        if !meta.is_dir() {
            return self
                .upload_one(sink, &session, local_path, remote_path, &name)
                .await;
        }

        // 目录：先扫描再逐个上传
        let files = scan_local_dir(local_path)?;
        let total: u64 = files.iter().map(|f| f.2).sum();
        let mut progress = TransferProgress {
            id: uuid::Uuid::new_v4().to_string(),
            file_name: format!("{name}/"),
            direction: "upload".into(),
            done: 0,
            total,
            status: "running".into(),
            message: Some(format!("共 {} 个文件", files.len())),
        };
        sink.progress(&progress);

        let mut created: HashSet<String> = HashSet::new();
        let root = remote_path.trim_end_matches('/').to_string();
        let result = async {
            ensure_remote_dir(&session, &root, &mut created).await?;
            for (abs, rel, _size) in files.iter() {
                let dst = join_remote(&root, rel);
                if let Some(parent) = dst.rfind('/').map(|i| dst[..i].to_string()) {
                    ensure_remote_dir(&session, &parent, &mut created).await?;
                }
                upload_file_stream(&session, abs, &dst, sink, &mut progress).await?;
            }
            Ok::<(), Error>(())
        }
        .await;
        finish_progress(sink, &mut progress, result)
    }

    async fn upload_one(
        &self,
        sink: &dyn TransferSink,
        session: &SftpSession,
        local_path: &str,
        remote_path: &str,
        name: &str,
    ) -> Result<()> {
        let mut progress = TransferProgress {
            id: uuid::Uuid::new_v4().to_string(),
            file_name: name.to_string(),
            direction: "upload".into(),
            done: 0,
            total: std::fs::metadata(local_path).map(|m| m.len()).unwrap_or(0),
            status: "running".into(),
            message: None,
        };
        sink.progress(&progress);
        let result = upload_file_stream(session, local_path, remote_path, sink, &mut progress).await;
        finish_progress(sink, &mut progress, result)
    }

    // ------------------------------------------------------------ 下载

    /// 下载远程文件或整个目录（`local_path` 为目标路径本身）
    pub async fn download(
        &self,
        sink: &dyn TransferSink,
        id: &str,
        remote_path: &str,
        local_path: &str,
    ) -> Result<()> {
        let session = self.session(id)?;
        let meta = session
            .metadata(remote_path)
            .await
            .map_err(|e| Error::msg(format!("无法访问远程路径（{remote_path}）：{e}")))?;
        let name = basename_any(remote_path);

        if !meta.file_type().is_dir() {
            return self
                .download_one(sink, &session, remote_path, local_path, &name)
                .await;
        }

        let files = scan_remote_dir(&session, remote_path).await?;
        let total: u64 = files.iter().map(|f| f.1).sum();
        let mut progress = TransferProgress {
            id: uuid::Uuid::new_v4().to_string(),
            file_name: format!("{name}/"),
            direction: "download".into(),
            done: 0,
            total,
            status: "running".into(),
            message: Some(format!("共 {} 个文件", files.len())),
        };
        sink.progress(&progress);

        let root = std::path::PathBuf::from(local_path);
        let result = async {
            std::fs::create_dir_all(&root)
                .map_err(|e| Error::msg(format!("创建本地文件夹失败（{}）：{e}", root.display())))?;
            for (remote, _size) in files.iter() {
                // 远程路径相对根目录的部分
                let rel = remote
                    .strip_prefix(remote_path)
                    .unwrap_or(remote)
                    .trim_start_matches('/');
                let target = root.join(rel.replace('/', &std::path::MAIN_SEPARATOR.to_string()));
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        Error::msg(format!("创建本地文件夹失败（{}）：{e}", parent.display()))
                    })?;
                }
                download_file_stream(
                    &session,
                    remote,
                    &target.to_string_lossy(),
                    sink,
                    &mut progress,
                )
                .await?;
            }
            Ok::<(), Error>(())
        }
        .await;
        finish_progress(sink, &mut progress, result)
    }

    async fn download_one(
        &self,
        sink: &dyn TransferSink,
        session: &SftpSession,
        remote_path: &str,
        local_path: &str,
        name: &str,
    ) -> Result<()> {
        let mut progress = TransferProgress {
            id: uuid::Uuid::new_v4().to_string(),
            file_name: name.to_string(),
            direction: "download".into(),
            done: 0,
            total: session
                .metadata(remote_path)
                .await
                .map(|m| m.len())
                .unwrap_or(0),
            status: "running".into(),
            message: None,
        };
        sink.progress(&progress);
        let result = download_file_stream(session, remote_path, local_path, sink, &mut progress).await;
        finish_progress(sink, &mut progress, result)
    }

    // ------------------------------------------------------------ 远程 → 远程

    /// 两台远程主机之间直接对流传输（不落地本地磁盘），支持整个目录
    pub async fn copy_remote(
        &self,
        sink: &dyn TransferSink,
        src_id: &str,
        src_path: &str,
        dst_id: &str,
        dst_path: &str,
    ) -> Result<()> {
        let src = self.session(src_id)?;
        let dst = self.session(dst_id)?;
        let meta = src
            .metadata(src_path)
            .await
            .map_err(|e| Error::msg(format!("无法访问源路径（{src_path}）：{e}")))?;
        let name = basename_any(src_path);

        if !meta.file_type().is_dir() {
            let mut progress = TransferProgress {
                id: uuid::Uuid::new_v4().to_string(),
                file_name: name,
                direction: "remoteToRemote".into(),
                done: 0,
                total: meta.len(),
                status: "running".into(),
                message: None,
            };
            sink.progress(&progress);
            let result = copy_stream(&src, src_path, &dst, dst_path, sink, &mut progress).await;
            return finish_progress(sink, &mut progress, result);
        }

        let files = scan_remote_dir(&src, src_path).await?;
        let total: u64 = files.iter().map(|f| f.1).sum();
        let mut progress = TransferProgress {
            id: uuid::Uuid::new_v4().to_string(),
            file_name: format!("{name}/"),
            direction: "remoteToRemote".into(),
            done: 0,
            total,
            status: "running".into(),
            message: Some(format!("共 {} 个文件", files.len())),
        };
        sink.progress(&progress);

        let root = dst_path.trim_end_matches('/').to_string();
        let mut created: HashSet<String> = HashSet::new();
        let result = async {
            ensure_remote_dir(&dst, &root, &mut created).await?;
            for (remote, _size) in files.iter() {
                let rel = remote
                    .strip_prefix(src_path)
                    .unwrap_or(remote)
                    .trim_start_matches('/');
                let target = join_remote(&root, rel);
                if let Some(parent) = target.rfind('/').map(|i| target[..i].to_string()) {
                    ensure_remote_dir(&dst, &parent, &mut created).await?;
                }
                copy_stream(&src, remote, &dst, &target, sink, &mut progress).await?;
            }
            Ok::<(), Error>(())
        }
        .await;
        finish_progress(sink, &mut progress, result)
    }
}

// ---------------------------------------------------------------- 流式读写工具

fn finish_progress(
    sink: &dyn TransferSink,
    progress: &mut TransferProgress,
    result: Result<()>,
) -> Result<()> {
    match result {
        Ok(()) => {
            progress.status = "done".into();
            progress.message = None;
            sink.progress(progress);
            Ok(())
        }
        Err(e) => {
            progress.status = "error".into();
            progress.message = Some(e.to_string());
            sink.progress(progress);
            Err(e)
        }
    }
}

async fn upload_file_stream(
    session: &SftpSession,
    local_path: &str,
    remote_path: &str,
    sink: &dyn TransferSink,
    progress: &mut TransferProgress,
) -> Result<()> {
    let mut local = tokio::fs::File::open(local_path)
        .await
        .map_err(|e| Error::msg(format!("打开本地文件失败（{local_path}）：{e}")))?;
    let mut remote = session
        .create(remote_path)
        .await
        .map_err(|e| Error::msg(format!("创建远程文件失败（{remote_path}）：{e}")))?;

    let mut buf = vec![0u8; CHUNK];
    let mut last = Instant::now();
    loop {
        let n = local
            .read(&mut buf)
            .await
            .map_err(|e| Error::msg(format!("读取本地文件失败（{local_path}）：{e}")))?;
        if n == 0 {
            break;
        }
        remote
            .write_all(&buf[..n])
            .await
            .map_err(|e| Error::msg(format!("写入远程文件失败（{remote_path}）：{e}")))?;
        progress.done += n as u64;
        if last.elapsed() > Duration::from_millis(120) {
            last = Instant::now();
            sink.progress(progress);
        }
    }
    let _ = remote.shutdown().await;
    Ok(())
}

async fn download_file_stream(
    session: &SftpSession,
    remote_path: &str,
    local_path: &str,
    sink: &dyn TransferSink,
    progress: &mut TransferProgress,
) -> Result<()> {
    let mut remote = session
        .open(remote_path)
        .await
        .map_err(|e| Error::msg(format!("打开远程文件失败（{remote_path}）：{e}")))?;
    if let Some(parent) = std::path::Path::new(local_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut local = tokio::fs::File::create(local_path)
        .await
        .map_err(|e| Error::msg(format!("创建本地文件失败（{local_path}）：{e}")))?;

    let mut buf = vec![0u8; CHUNK];
    let mut last = Instant::now();
    loop {
        let n = remote
            .read(&mut buf)
            .await
            .map_err(|e| Error::msg(format!("读取远程文件失败（{remote_path}）：{e}")))?;
        if n == 0 {
            break;
        }
        local
            .write_all(&buf[..n])
            .await
            .map_err(|e| Error::msg(format!("写入本地文件失败（{local_path}）：{e}")))?;
        progress.done += n as u64;
        if last.elapsed() > Duration::from_millis(120) {
            last = Instant::now();
            sink.progress(progress);
        }
    }
    let _ = local.flush().await;
    Ok(())
}

async fn copy_stream(
    src: &SftpSession,
    src_path: &str,
    dst: &SftpSession,
    dst_path: &str,
    sink: &dyn TransferSink,
    progress: &mut TransferProgress,
) -> Result<()> {
    let mut reader = src
        .open(src_path)
        .await
        .map_err(|e| Error::msg(format!("打开源文件失败（{src_path}）：{e}")))?;
    let mut writer = dst
        .create(dst_path)
        .await
        .map_err(|e| Error::msg(format!("创建目标文件失败（{dst_path}）：{e}")))?;

    let mut buf = vec![0u8; CHUNK];
    let mut last = Instant::now();
    loop {
        let n = reader
            .read(&mut buf)
            .await
            .map_err(|e| Error::msg(format!("读取源文件失败（{src_path}）：{e}")))?;
        if n == 0 {
            break;
        }
        writer
            .write_all(&buf[..n])
            .await
            .map_err(|e| Error::msg(format!("写入目标文件失败（{dst_path}）：{e}")))?;
        progress.done += n as u64;
        if last.elapsed() > Duration::from_millis(120) {
            last = Instant::now();
            sink.progress(progress);
        }
    }
    let _ = writer.shutdown().await;
    Ok(())
}

pub fn normalize_remote(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn basename(path: &str, sep: char) -> String {
    let s = path.trim_end_matches(sep);
    match s.rfind(sep) {
        Some(idx) if idx + 1 < s.len() => s[idx + 1..].to_string(),
        _ => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_remote_paths() {
        assert_eq!(join_remote("/", "a"), "/a");
        assert_eq!(join_remote("/home/u", "a"), "/home/u/a");
        assert_eq!(join_remote("/home/u/", "a"), "/home/u/a");
        assert_eq!(join_remote("/home/u", "sub/a"), "/home/u/sub/a");
    }

    #[test]
    fn parents() {
        assert_eq!(parent_remote("/a/b/c"), "/a/b");
        assert_eq!(parent_remote("/a"), "/");
        assert_eq!(parent_remote("/a/"), "/");
    }

    #[test]
    fn basenames() {
        assert_eq!(basename_any(r"C:\Users\x\项目"), "项目");
        assert_eq!(basename_any("/var/log/system.log"), "system.log");
        assert_eq!(basename_any("/var/log/"), "log");
    }

    #[test]
    fn scans_local_dir_recursively() {
        let dir = std::env::temp_dir().join(format!("tsftp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("sub/deep")).unwrap();
        std::fs::write(dir.join("a.txt"), b"1").unwrap();
        std::fs::write(dir.join("sub/b.txt"), b"22").unwrap();
        std::fs::write(dir.join("sub/deep/c.txt"), b"333").unwrap();

        let files = scan_local_dir(&dir.to_string_lossy()).unwrap();
        let rels: Vec<&str> = files.iter().map(|f| f.1.as_str()).collect();
        assert_eq!(rels, vec!["a.txt", "sub/b.txt", "sub/deep/c.txt"]);
        assert_eq!(files.iter().map(|f| f.2).sum::<u64>(), 6);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod e2e {
    use super::*;
    use crate::models::{AuthConfig, AUTH_PASSWORD};

    /// 收集进度事件，代替 tauri 的 AppHandle
    struct Collecting(std::sync::Mutex<Vec<TransferProgress>>);
    impl TransferSink for Collecting {
        fn progress(&self, p: &TransferProgress) {
            self.0.lock().unwrap().push(p.clone());
        }
    }

    fn test_host(port: u16) -> Host {
        Host {
            id: "e2e".into(),
            label: "e2e".into(),
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

    /// 需要一台真实 SSH 服务器，例如：
    ///   docker run -d --name tsssh -p 2222:22 alpine:3.20 sh -c "apk add --no-cache openssh-server; echo root:tomato123 | chpasswd; ssh-keygen -A; sed -i 's/^#*PermitRootLogin.*/PermitRootLogin yes/' /etc/ssh/sshd_config; /usr/sbin/sshd -D -e"
    ///   TOMATO_TEST_SSH=127.0.0.1:2222 cargo test --lib sftp::e2e -- --nocapture
    #[test]
    fn directory_upload_download_copy_and_remove() {
        let Ok(addr) = std::env::var("TOMATO_TEST_SSH") else {
            eprintln!("跳过 SFTP 端到端测试：未设置 TOMATO_TEST_SSH=127.0.0.1:2222");
            return;
        };
        let port: u16 = addr.rsplit(':').next().unwrap().parse().expect("端口");
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async move {
            let sink = Collecting(std::sync::Mutex::new(Vec::new()));
            let mgr = SftpManager::default();
            let host = test_host(port);
            let opened = mgr.open("s1", &host).await.expect("连接 SFTP");
            let home = opened.home.clone();
            let tag = uuid::Uuid::new_v4().to_string();

            // 1) 造一个多层目录
            let src = std::env::temp_dir().join(format!("tsftp-src-{tag}"));
            std::fs::create_dir_all(src.join("sub/deep")).unwrap();
            std::fs::write(src.join("a.txt"), b"1111").unwrap();
            std::fs::write(src.join("sub/b.txt"), b"22222").unwrap();
            std::fs::write(src.join("sub/deep/c.bin"), vec![7u8; 5000]).unwrap();

            // 2) 递归上传
            let remote_dir = join_remote(&home, &format!("tsftp-{tag}"));
            mgr.upload(&sink, "s1", &src.to_string_lossy(), &remote_dir)
                .await
                .expect("上传目录");
            let session = mgr.session("s1").unwrap();
            let files = scan_remote_dir(&session, &remote_dir).await.unwrap();
            let mut names: Vec<String> = files
                .iter()
                .map(|f| f.0.trim_start_matches(&remote_dir).to_string())
                .collect();
            names.sort();
            assert_eq!(names, vec!["/a.txt", "/sub/b.txt", "/sub/deep/c.bin"]);

            // 3) 递归下载
            let dl = std::env::temp_dir().join(format!("tsftp-dl-{tag}"));
            mgr.download(&sink, "s1", &remote_dir, &dl.to_string_lossy())
                .await
                .expect("下载目录");
            assert_eq!(std::fs::read(dl.join("a.txt")).unwrap(), b"1111");
            assert_eq!(std::fs::read(dl.join("sub/b.txt")).unwrap(), b"22222");
            assert_eq!(std::fs::read(dl.join("sub/deep/c.bin")).unwrap().len(), 5000);

            // 4) 远程 → 远程（目录）
            mgr.open("s2", &host).await.expect("第二个 SFTP 会话");
            let copy_to = join_remote(&home, &format!("tsftp-copy-{tag}"));
            mgr.copy_remote(&sink, "s1", &remote_dir, "s2", &copy_to)
                .await
                .expect("远程互传");
            let session2 = mgr.session("s2").unwrap();
            assert_eq!(scan_remote_dir(&session2, &copy_to).await.unwrap().len(), 3);

            // 5) 递归删除（目录非空也能删）
            mgr.remove("s1", &remote_dir, true).await.expect("递归删除");
            mgr.remove("s2", &copy_to, true).await.expect("递归删除2");
            assert!(!session.try_exists(remote_dir.as_str()).await.unwrap());
            assert!(!session2.try_exists(copy_to.as_str()).await.unwrap());

            // 6) 进度事件
            let events = sink.0.lock().unwrap();
            assert!(events.iter().any(|e| e.status == "done"));
            assert!(events.iter().any(|e| e.done > 0));

            let _ = std::fs::remove_dir_all(&src);
            let _ = std::fs::remove_dir_all(&dl);
        });
    }
}