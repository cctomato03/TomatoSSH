use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::task::JoinSet;

use crate::error::{Error, Result};
use crate::models::{now_ms, TransferProgress};
use crate::sftp::{SftpManager, TransferSink};
use crate::vault::VaultStore;

/// 同时跑几个"任务"（每个任务内部还有 4 路文件并发）
const JOB_CONCURRENCY: usize = 2;
/// 任务级重试次数（连接断开等致命错误时重来，文件级续传保证不重传已完成的字节）
const MAX_JOB_ATTEMPTS: u32 = 2;

/// 队列里的一个传输任务
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TransferJob {
    pub id: String,
    pub name: String,
    /// upload | download | remoteToRemote | localToLocal
    pub kind: String,
    #[serde(default)]
    pub src_host: Option<String>,
    pub src_path: String,
    #[serde(default)]
    pub dst_host: Option<String>,
    pub dst_path: String,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub done: u64,
    /// pending | running | done | error | canceled
    pub status: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub created_at: i64,
}

/// 前端入队请求
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueJob {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub src_host: Option<String>,
    pub src_path: String,
    #[serde(default)]
    pub dst_host: Option<String>,
    pub dst_path: String,
}

struct Inner {
    jobs: Mutex<Vec<TransferJob>>,
    store_path: PathBuf,
    app: AppHandle,
    sftp: Arc<SftpManager>,
    vault: Arc<Mutex<VaultStore>>,
    notify: tokio::sync::Notify,
}

#[derive(Clone)]
pub struct TransferQueue {
    inner: Arc<Inner>,
}

/// 把传输过程里的进度写回队列项
struct JobSink {
    queue: TransferQueue,
    job_id: String,
}

impl TransferSink for JobSink {
    fn progress(&self, p: &TransferProgress) {
        self.queue.apply_progress(&self.job_id, p);
    }
}

impl TransferQueue {
    pub fn new(
        app: AppHandle,
        sftp: Arc<SftpManager>,
        vault: Arc<Mutex<VaultStore>>,
        store_path: PathBuf,
    ) -> Self {
        let queue = Self {
            inner: Arc::new(Inner {
                jobs: Mutex::new(Vec::new()),
                store_path,
                app,
                sftp,
                vault,
                notify: tokio::sync::Notify::new(),
            }),
        };
        queue.load_from_disk();
        queue
    }

    fn load_from_disk(&self) {
        let path = &self.inner.store_path;
        if !path.exists() {
            return;
        }
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => return,
        };
        let mut jobs: Vec<TransferJob> = serde_json::from_str(&text).unwrap_or_default();
        // 上次退出时正在跑的任务，重启后重新排队（配合断点续传继续）
        for j in jobs.iter_mut() {
            if j.status == "running" {
                j.status = "pending".into();
                j.message = Some("上次退出时未完成，已重新排队（断点续传）".into());
            }
        }
        if let Ok(mut guard) = self.inner.jobs.lock() {
            *guard = jobs;
        }
    }

    fn persist(&self) {
        let jobs = self.list();
        if let Ok(text) = serde_json::to_string_pretty(&jobs) {
            let _ = std::fs::write(&self.inner.store_path, text);
        }
    }

    fn emit(&self, job: &TransferJob) {
        let _ = self.inner.app.emit("transfer-job", job.clone());
        // 兼容主窗口的传输进度条
        let _ = self.inner.app.emit(
            "transfer-progress",
            TransferProgress {
                id: job.id.clone(),
                file_name: job.name.clone(),
                direction: job.kind.clone(),
                done: job.done,
                total: job.total,
                status: match job.status.as_str() {
                    "done" => "done".into(),
                    "error" => "error".into(),
                    _ => "running".into(),
                },
                message: job.message.clone(),
            },
        );
    }

    pub fn list(&self) -> Vec<TransferJob> {
        self.inner
            .jobs
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    pub fn enqueue(&self, items: Vec<EnqueueJob>) -> Vec<TransferJob> {
        let mut created = Vec::new();
        if let Ok(mut guard) = self.inner.jobs.lock() {
            for it in items {
                let job = TransferJob {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: it.name,
                    kind: it.kind,
                    src_host: it.src_host,
                    src_path: it.src_path,
                    dst_host: it.dst_host,
                    dst_path: it.dst_path,
                    total: 0,
                    done: 0,
                    status: "pending".into(),
                    message: None,
                    attempts: 0,
                    created_at: now_ms(),
                };
                guard.push(job.clone());
                created.push(job);
            }
        }
        self.persist();
        for j in &created {
            self.emit(j);
        }
        self.inner.notify.notify_waiters();
        created
    }

    pub fn cancel(&self, id: &str) {
        if let Ok(mut guard) = self.inner.jobs.lock() {
            if let Some(j) = guard.iter_mut().find(|j| j.id == id) {
                if matches!(j.status.as_str(), "pending" | "running") {
                    j.status = "canceled".into();
                    j.message = Some("已取消".into());
                }
            }
        }
        self.persist();
        if let Some(j) = self.list().into_iter().find(|j| j.id == id) {
            self.emit(&j);
        }
    }

    pub fn retry(&self, id: &str) {
        if let Ok(mut guard) = self.inner.jobs.lock() {
            if let Some(j) = guard.iter_mut().find(|j| j.id == id) {
                if matches!(j.status.as_str(), "error" | "canceled") {
                    j.status = "pending".into();
                    j.attempts = 0;
                    j.message = Some("重新排队（断点续传）".into());
                }
            }
        }
        self.persist();
        self.inner.notify.notify_waiters();
        if let Some(j) = self.list().into_iter().find(|j| j.id == id) {
            self.emit(&j);
        }
    }

    pub fn clear_finished(&self) {
        if let Ok(mut guard) = self.inner.jobs.lock() {
            guard.retain(|j| !matches!(j.status.as_str(), "done" | "canceled" | "error"));
        }
        self.persist();
    }

    fn apply_progress(&self, job_id: &str, p: &TransferProgress) {
        let updated = if let Ok(mut guard) = self.inner.jobs.lock() {
            if let Some(j) = guard.iter_mut().find(|j| j.id == job_id) {
                j.done = p.done;
                if p.total > 0 {
                    j.total = p.total;
                }
                if p.message.is_some() {
                    j.message = p.message.clone();
                }
                Some(j.clone())
            } else {
                None
            }
        } else {
            None
        };
        if let Some(j) = updated {
            self.emit(&j);
        }
    }

    fn claim_pending(&self, slots: usize) -> Vec<TransferJob> {
        let mut picked = Vec::new();
        if let Ok(mut guard) = self.inner.jobs.lock() {
            for j in guard.iter_mut() {
                if picked.len() >= slots {
                    break;
                }
                if j.status == "pending" {
                    j.status = "running".into();
                    j.message = Some("正在传输…".into());
                    picked.push(j.clone());
                }
            }
        }
        if !picked.is_empty() {
            self.persist();
            for j in &picked {
                self.emit(j);
            }
        }
        picked
    }

    /// 后台常驻的调度循环：关闭 UI 窗口后依然继续传输
    pub fn start(&self) {
        let queue = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut set: JoinSet<()> = JoinSet::new();
            loop {
                while set.try_join_next().is_some() {}
                let slots = JOB_CONCURRENCY.saturating_sub(set.len());
                if slots > 0 {
                    for job in queue.claim_pending(slots) {
                        let q = queue.clone();
                        set.spawn(async move {
                            q.run_job(job).await;
                        });
                    }
                }
                tokio::select! {
                    Some(_) = set.join_next() => {},
                    _ = queue.inner.notify.notified() => {},
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {},
                }
            }
        });
    }

    /// 取主机信息（用于自己建立 SFTP 会话，不依赖文件管理器窗口）
    fn host_of(&self, host_id: &str) -> Result<crate::models::Host> {
        let vault = self
            .inner
            .vault
            .lock()
            .map_err(|_| Error::msg("保险库锁错误"))?;
        vault.host(host_id)
    }

    async fn session_for(&self, job_id: &str, role: &str, host_id: &str) -> Result<String> {
        let key = format!("queue:{job_id}:{role}");
        let host = self.host_of(host_id)?;
        // 支持经跳板机
        let jump = host
            .jump_host_id
            .as_deref()
            .filter(|id| !id.is_empty() && *id != host.id)
            .and_then(|id| self.host_of(id).ok());
        self.inner.sftp.open(&key, &host, jump.as_ref()).await?;
        Ok(key)
    }

    async fn run_job(&self, job: TransferJob) {
        // 任务在排队期间可能被取消
        if self
            .list()
            .into_iter()
            .find(|j| j.id == job.id)
            .map(|j| j.status != "running")
            .unwrap_or(true)
        {
            return;
        }

        let sink = JobSink {
            queue: self.clone(),
            job_id: job.id.clone(),
        };

        let mut keys: Vec<String> = Vec::new();
        let result: Result<()> = match job.kind.as_str() {
            "upload" => {
                let dst_host = job.dst_host.clone().unwrap_or_default();
                match self.session_for(&job.id, "dst", &dst_host).await {
                    Ok(k) => {
                        keys.push(k.clone());
                        self.inner
                            .sftp
                            .upload(&sink, &k, &job.src_path, &job.dst_path)
                            .await
                    }
                    Err(e) => Err(e),
                }
            }
            "download" => {
                let src_host = job.src_host.clone().unwrap_or_default();
                match self.session_for(&job.id, "src", &src_host).await {
                    Ok(k) => {
                        keys.push(k.clone());
                        self.inner
                            .sftp
                            .download(&sink, &k, &job.src_path, &job.dst_path)
                            .await
                    }
                    Err(e) => Err(e),
                }
            }
            "remoteToRemote" => {
                let src_host = job.src_host.clone().unwrap_or_default();
                let dst_host = job.dst_host.clone().unwrap_or_default();
                let src_key = self.session_for(&job.id, "src", &src_host).await;
                let dst_key = self.session_for(&job.id, "dst", &dst_host).await;
                match (src_key, dst_key) {
                    (Ok(a), Ok(b)) => {
                        keys.push(a.clone());
                        keys.push(b.clone());
                        self.inner
                            .sftp
                            .copy_remote(&sink, &a, &job.src_path, &b, &job.dst_path)
                            .await
                    }
                    (Err(e), _) | (_, Err(e)) => Err(e),
                }
            }
            "localToLocal" => {
                let (from, to) = (job.src_path.clone(), job.dst_path.clone());
                match tokio::task::spawn_blocking(move || crate::localfs::copy(&from, &to)).await {
                    Ok(r) => r,
                    Err(e) => Err(Error::msg(format!("本地复制任务失败：{e}"))),
                }
            }
            other => Err(Error::msg(format!("未知的传输类型：{other}"))),
        };

        // 关闭本任务临时用的会话
        for k in &keys {
            self.inner.sftp.close(k);
        }

        // 收集最终进度（文件传输会持续推送进度事件）
        let finished = self.finish(job.id.clone(), result);

        // 任务级重试：连接类错误时重新排队，靠断点续传接着传
        if finished.status == "error" && finished.attempts < MAX_JOB_ATTEMPTS {
            if let Ok(mut guard) = self.inner.jobs.lock() {
                if let Some(j) = guard.iter_mut().find(|j| j.id == finished.id) {
                    if j.status == "error" {
                        j.attempts += 1;
                        j.status = "pending".into();
                        j.message = Some(format!(
                            "第 {} 次失败，正在重试（断点续传）：{}",
                            j.attempts,
                            j.message.clone().unwrap_or_default()
                        ));
                    }
                }
            }
            self.persist();
            self.inner.notify.notify_waiters();
        }
    }

    fn finish(&self, job_id: String, result: Result<()>) -> TransferJob {
        let updated = if let Ok(mut guard) = self.inner.jobs.lock() {
            if let Some(j) = guard.iter_mut().find(|j| j.id == job_id) {
                if j.status == "canceled" {
                    // 已被取消，保持取消状态
                    Some(j.clone())
                } else {
                    match &result {
                        Ok(()) => {
                            j.status = "done".into();
                            j.done = j.total.max(j.done);
                            j.message = None;
                        }
                        Err(e) => {
                            j.status = "error".into();
                            j.message = Some(e.to_string());
                        }
                    }
                    Some(j.clone())
                }
            } else {
                None
            }
        } else {
            None
        };
        self.persist();
        if let Some(j) = &updated {
            self.emit(j);
        }
        updated.unwrap_or_else(|| TransferJob {
            id: job_id,
            name: String::new(),
            kind: String::new(),
            src_host: None,
            src_path: String::new(),
            dst_host: None,
            dst_path: String::new(),
            total: 0,
            done: 0,
            status: "error".into(),
            message: Some("任务丢失".into()),
            attempts: 0,
            created_at: now_ms(),
        })
    }
}

/// 供前端展示的队列快照（含各状态计数）
pub fn summary(jobs: &[TransferJob]) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    for j in jobs {
        *map.entry(j.status.clone()).or_insert(0) += 1;
    }
    map
}
