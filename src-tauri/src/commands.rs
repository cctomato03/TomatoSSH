use std::sync::{Arc, Mutex};

use tauri::{AppHandle, State};

use crate::error::{Error, Result};
use crate::local::LocalManager;
use crate::localfs;
use crate::models::*;
use crate::queue::{EnqueueJob, TransferJob, TransferQueue};
use crate::serial::SerialManager;
use crate::sftp::SftpManager;
use crate::ssh::SshManager;
use crate::sync;
use crate::vault::VaultStore;
use crate::{crypto, vault::SyncPayload};

pub struct AppState {
    pub vault: Arc<Mutex<VaultStore>>,
    pub ssh: Arc<SshManager>,
    pub sftp: Arc<SftpManager>,
    pub local: Arc<LocalManager>,
    pub serial: Arc<SerialManager>,
    pub queue: Arc<TransferQueue>,
}

impl AppState {
    pub fn new(
        vault: Arc<Mutex<VaultStore>>,
        ssh: Arc<SshManager>,
        sftp: Arc<SftpManager>,
        local: Arc<LocalManager>,
        serial: Arc<SerialManager>,
        queue: Arc<TransferQueue>,
    ) -> Self {
        Self {
            vault,
            ssh,
            sftp,
            local,
            serial,
            queue,
        }
    }
}

// ---------------------------------------------------------------- 保险库 / 连接

#[tauri::command]
pub fn vault_load(state: State<'_, AppState>) -> Result<Snapshot> {
    let vault = state.vault.lock()?;
    Ok(vault.snapshot())
}

#[tauri::command]
pub fn host_save(state: State<'_, AppState>, draft: HostDraft) -> Result<Host> {
    let mut vault = state.vault.lock()?;
    vault.upsert_host(draft)
}

#[tauri::command]
pub fn host_delete(state: State<'_, AppState>, id: String) -> Result<()> {
    let mut vault = state.vault.lock()?;
    vault.delete_host(&id)
}

#[tauri::command]
pub fn host_clear_fingerprint(state: State<'_, AppState>, id: String) -> Result<()> {
    let mut vault = state.vault.lock()?;
    vault.clear_host_fingerprint(&id)
}

#[tauri::command]
pub fn group_save(state: State<'_, AppState>, draft: GroupDraft) -> Result<Group> {    let mut vault = state.vault.lock()?;
    vault.upsert_group(draft)
}

#[tauri::command]
pub fn group_delete(state: State<'_, AppState>, id: String) -> Result<()> {
    let mut vault = state.vault.lock()?;
    vault.delete_group(&id)
}

#[tauri::command]
pub fn settings_save(state: State<'_, AppState>, settings: Settings) -> Result<()> {
    let mut vault = state.vault.lock()?;
    vault.set_settings(settings)
}

#[tauri::command]
pub fn ssh_target(state: State<'_, AppState>, host_id: String) -> Result<SshTarget> {
    let vault = state.vault.lock()?;
    let host = vault.host(&host_id)?;
    Ok(SshTarget {
        host: host.host,
        username: host.username,
    })
}

// ---------------------------------------------------------------- 终端

#[tauri::command]
pub async fn ssh_open(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    host_id: String,
    cols: u32,
    rows: u32,
) -> Result<String> {
    let host = {
        let vault = state.vault.lock()?;
        vault.host(&host_id)?
    };

    let opened = state
        .ssh
        .open_terminal(app, session_id, host.clone(), cols, rows)
        .await?;

    // TOFU：首次连接记录服务器公钥指纹
    if host.host_key_fingerprint.is_none() {
        if let Some(fp) = opened.fingerprint.as_deref() {
            let mut vault = state.vault.lock()?;
            let _ = vault.set_host_fingerprint(&host.id, fp);
        }
    }
    Ok(opened.banner)
}

#[tauri::command]
pub fn local_open(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
    cwd: Option<String>,
) -> Result<String> {
    state.local.open(app, session_id, cols, rows, cwd)
}

#[tauri::command]
pub async fn term_write(state: State<'_, AppState>, session_id: String, data: String) -> Result<()> {
    if state.ssh.has_session(&session_id) {
        state.ssh.write(&session_id, data.as_bytes()).await
    } else if state.local.has(&session_id) {
        state.local.write(&session_id, data.as_bytes())
    } else if state.serial.has(&session_id) {
        state.serial.write(&session_id, data.as_bytes())
    } else {
        Err(Error::msg("会话不存在或已关闭"))
    }
}

#[tauri::command]
pub async fn term_resize(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<()> {
    if state.ssh.has_session(&session_id) {
        state.ssh.resize(&session_id, cols as u32, rows as u32).await
    } else if state.local.has(&session_id) {
        state.local.resize(&session_id, cols, rows)
    } else {
        Ok(())
    }
}

#[tauri::command]
pub async fn term_close(state: State<'_, AppState>, session_id: String) -> Result<()> {
    if state.ssh.has_session(&session_id) {
        state.ssh.close(&session_id).await?;
    }
    if state.local.has(&session_id) {
        state.local.close(&session_id);
    }
    if state.serial.has(&session_id) {
        state.serial.close(&session_id);
    }
    Ok(())
}

// ---------------------------------------------------------------- 远程文件

#[tauri::command]
pub async fn sftp_open(
    state: State<'_, AppState>,
    session_id: String,
    host_id: String,
) -> Result<String> {
    let host = {
        let vault = state.vault.lock()?;
        vault.host(&host_id)?
    };
    let opened = state.sftp.open(&session_id, &host).await?;
    if host.host_key_fingerprint.is_none() {
        if let Some(fp) = opened.fingerprint.as_deref() {
            let mut vault = state.vault.lock()?;
            let _ = vault.set_host_fingerprint(&host.id, fp);
        }
    }
    Ok(opened.home)
}

#[tauri::command]
pub async fn sftp_close(state: State<'_, AppState>, session_id: String) -> Result<()> {
    state.sftp.close(&session_id);
    Ok(())
}

#[tauri::command]
pub async fn sftp_home(state: State<'_, AppState>, session_id: String) -> Result<String> {
    state.sftp.home(&session_id).await
}

#[tauri::command]
pub async fn sftp_list(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<Vec<FileEntry>> {
    state.sftp.list(&session_id, &path).await
}

#[tauri::command]
pub async fn sftp_mkdir(state: State<'_, AppState>, session_id: String, path: String) -> Result<()> {
    state.sftp.mkdir(&session_id, &path).await
}

#[tauri::command]
pub async fn sftp_remove(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    is_dir: bool,
) -> Result<()> {
    state.sftp.remove(&session_id, &path, is_dir).await
}

#[tauri::command]
pub async fn sftp_rename(
    state: State<'_, AppState>,
    session_id: String,
    from: String,
    to: String,
) -> Result<()> {
    state.sftp.rename(&session_id, &from, &to).await
}

#[tauri::command]
pub async fn sftp_download(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    remote_path: String,
    local_path: String,
) -> Result<()> {
    state
        .sftp
        .download(&app, &session_id, &remote_path, &local_path)
        .await
}

#[tauri::command]
pub async fn sftp_upload(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    local_path: String,
    remote_path: String,
) -> Result<()> {
    state
        .sftp
        .upload(&app, &session_id, &local_path, &remote_path)
        .await
}

#[tauri::command]
pub async fn sftp_copy_remote(
    app: AppHandle,
    state: State<'_, AppState>,
    src_session_id: String,
    src_path: String,
    dst_session_id: String,
    dst_path: String,
) -> Result<()> {
    state
        .sftp
        .copy_remote(&app, &src_session_id, &src_path, &dst_session_id, &dst_path)
        .await
}

// ---------------------------------------------------------------- 本地文件

#[tauri::command]
pub fn local_list(path: Option<String>) -> Result<Vec<FileEntry>> {
    localfs::list(path)
}

#[tauri::command]
pub fn local_home() -> Result<LocalHome> {
    localfs::home()
}

#[tauri::command]
pub fn local_mkdir(path: String) -> Result<()> {
    localfs::mkdir(&path)
}

#[tauri::command]
pub fn local_remove(path: String) -> Result<()> {
    localfs::remove(&path)
}

#[tauri::command]
pub fn local_rename(from: String, to: String) -> Result<()> {
    localfs::rename(&from, &to)
}

#[tauri::command]
pub fn local_copy(from: String, to: String) -> Result<()> {
    localfs::copy(&from, &to)
}

// ---------------------------------------------------------------- 串口

#[tauri::command]
pub fn serial_list_ports() -> Result<Vec<SerialPortInfo>> {
    crate::serial::list_ports()
}

#[tauri::command]
pub fn serial_save(state: State<'_, AppState>, draft: SerialProfile) -> Result<SerialProfile> {
    let mut vault = state.vault.lock()?;
    vault.upsert_serial(draft)
}

#[tauri::command]
pub fn serial_delete(state: State<'_, AppState>, id: String) -> Result<()> {
    let mut vault = state.vault.lock()?;
    vault.delete_serial(&id)
}

/// 打开串口终端（只有终端交互，没有文件传输）
#[tauri::command]
pub fn serial_open(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    profile_id: String,
) -> Result<String> {
    let profile = {
        let vault = state.vault.lock()?;
        vault.serial(&profile_id)?
    };
    state.serial.open(app, session_id, &profile)
}

/// 打开独立的 SFTP 文件管理器窗口（支持 本地 ⇄ 远程、远程 ⇄ 远程）
#[tauri::command]
pub async fn open_file_manager(app: AppHandle, host_id: Option<String>) -> Result<()> {
    use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

    if let Some(win) = app.get_webview_window("files") {
        win.set_focus().ok();
        let _ = win.emit("fm-open", &host_id);
        return Ok(());
    }

    let url = match host_id.as_deref() {
        Some(h) if !h.is_empty() => format!("files.html?host={h}"),
        _ => "files.html".to_string(),
    };
    WebviewWindowBuilder::new(&app, "files", WebviewUrl::App(url.into()))
        .title("TomatoSSH 文件管理")
        .inner_size(1180.0, 760.0)
        .min_inner_size(760.0, 480.0)
        .build()
        .map_err(|e| Error::msg(format!("打开文件管理器窗口失败：{e}")))?;
    Ok(())
}

#[tauri::command]
pub fn app_version() -> Result<String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

// ---------------------------------------------------------------- 传输队列（后台常驻）

#[tauri::command]
pub fn enqueue_transfers(state: State<'_, AppState>, jobs: Vec<EnqueueJob>) -> Result<Vec<TransferJob>> {
    Ok(state.queue.enqueue(jobs))
}

#[tauri::command]
pub fn list_transfers(state: State<'_, AppState>) -> Result<Vec<TransferJob>> {
    Ok(state.queue.list())
}

#[tauri::command]
pub fn cancel_transfer(state: State<'_, AppState>, id: String) -> Result<()> {
    state.queue.cancel(&id);
    Ok(())
}

#[tauri::command]
pub fn retry_transfer(state: State<'_, AppState>, id: String) -> Result<()> {
    state.queue.retry(&id);
    Ok(())
}

#[tauri::command]
pub fn clear_finished_transfers(state: State<'_, AppState>) -> Result<()> {
    state.queue.clear_finished();
    Ok(())
}

// ---------------------------------------------------------------- 云同步

fn sync_state_of(vault: &VaultStore) -> SyncStateDto {
    let secrets = &vault.data.secrets;
    SyncStateDto {
        logged_in: secrets.sync_token.is_some(),
        email: if vault.data.settings.sync.email.is_empty() {
            None
        } else {
            Some(vault.data.settings.sync.email.clone())
        },
        server_url: if vault.data.settings.sync.server_url.is_empty() {
            None
        } else {
            Some(vault.data.settings.sync.server_url.clone())
        },
        revision: secrets.sync_revision,
        last_sync_at: vault.data.settings.sync.last_sync_at,
    }
}

#[tauri::command]
pub fn sync_state(state: State<'_, AppState>) -> Result<SyncStateDto> {
    let vault = state.vault.lock()?;
    Ok(sync_state_of(&vault))
}

#[tauri::command]
pub async fn sync_login(
    state: State<'_, AppState>,
    server_url: String,
    email: String,
    password: String,
) -> Result<SyncStateDto> {
    let auth = sync::login(&server_url, &email, &password).await?;
    let key = crypto::derive_sync_key(&password, &email);

    let mut vault = state.vault.lock()?;
    vault.data.secrets.sync_token = Some(auth.token);
    vault.data.secrets.sync_key = Some(crypto::key_to_b64(&key));
    vault.data.settings.sync.server_url = server_url.trim().trim_end_matches('/').to_string();
    vault.data.settings.sync.email = email;
    vault.save()?;
    Ok(sync_state_of(&vault))
}

#[tauri::command]
pub async fn sync_register(
    state: State<'_, AppState>,
    server_url: String,
    email: String,
    password: String,
) -> Result<SyncStateDto> {
    if password.len() < 6 {
        return Err(Error::msg("同步密码至少 6 位"));
    }
    let auth = sync::register(&server_url, &email, &password).await?;
    let key = crypto::derive_sync_key(&password, &email);

    let mut vault = state.vault.lock()?;
    vault.data.secrets.sync_token = Some(auth.token);
    vault.data.secrets.sync_key = Some(crypto::key_to_b64(&key));
    vault.data.settings.sync.server_url = server_url.trim().trim_end_matches('/').to_string();
    vault.data.settings.sync.email = email;
    vault.save()?;
    Ok(sync_state_of(&vault))
}

#[tauri::command]
pub fn sync_logout(state: State<'_, AppState>) -> Result<()> {
    let mut vault = state.vault.lock()?;
    vault.data.secrets.sync_token = None;
    vault.data.secrets.sync_key = None;
    vault.data.secrets.sync_revision = None;
    vault.save()
}

/// 组装当前连接、加密后的上传载荷
fn build_blob(vault: &VaultStore) -> Result<Vec<u8>> {
    let key_b64 = vault
        .data
        .secrets
        .sync_key
        .clone()
        .ok_or_else(|| Error::msg("尚未登录同步账号"))?;
    let key = crypto::key_from_b64(&key_b64)?;
    let payload = vault.payload();
    let plain = serde_json::to_vec(&payload)?;
    crypto::encrypt(&key, &plain)
}

#[tauri::command]
pub async fn sync_push(state: State<'_, AppState>) -> Result<SyncResult> {
    do_push(&state).await
}

async fn do_push(st: &AppState) -> Result<SyncResult> {
    let (server, token, blob, device_id, revision, count) = {
        let vault = st.vault.lock()?;
        let server = vault.data.settings.sync.server_url.clone();
        let token = vault
            .data
            .secrets
            .sync_token
            .clone()
            .ok_or_else(|| Error::msg("尚未登录同步账号"))?;
        if server.trim().is_empty() {
            return Err(Error::msg("请先填写同步服务器地址"));
        }
        let blob = build_blob(&vault)?;
        let count = vault.data.hosts.iter().filter(|h| !h.deleted).count();
        (
            server,
            token,
            blob,
            vault.data.secrets.device_id.clone(),
            vault.data.secrets.sync_revision,
            count,
        )
    };

    let new_revision = sync::push(&server, &token, &blob, &device_id, revision).await?;

    let mut vault = st.vault.lock()?;
    vault.data.secrets.sync_revision = Some(new_revision);
    vault.data.settings.sync.last_sync_at = Some(now_ms());
    vault.save()?;

    Ok(SyncResult {
        revision: new_revision,
        uploaded: count,
        downloaded: 0,
        message: format!("已上传 {count} 个连接（版本 {new_revision}）"),
    })
}

#[tauri::command]
pub async fn sync_pull(state: State<'_, AppState>) -> Result<SyncResult> {
    do_pull(&state).await
}

async fn do_pull(st: &AppState) -> Result<SyncResult> {
    let (server, token) = {
        let vault = st.vault.lock()?;
        (
            vault.data.settings.sync.server_url.clone(),
            vault
                .data
                .secrets
                .sync_token
                .clone()
                .ok_or_else(|| Error::msg("尚未登录同步账号"))?,
        )
    };

    let remote = sync::pull(&server, &token).await?;
    let Some((revision, blob)) = remote else {
        return Ok(SyncResult {
            revision: 0,
            uploaded: 0,
            downloaded: 0,
            message: "云端暂无数据，请先上传".into(),
        });
    };

    let key_b64 = {
        let vault = st.vault.lock()?;
        vault
            .data
            .secrets
            .sync_key
            .clone()
            .ok_or_else(|| Error::msg("本机缺少同步密钥，请重新登录同步账号"))?
    };
    let key = crypto::key_from_b64(&key_b64)?;
    let plain = crypto::decrypt(&key, &blob)?;
    let payload: SyncPayload = serde_json::from_slice(&plain)
        .map_err(|e| Error::msg(format!("云端数据格式不正确：{e}")))?;

    let changed = {
        let mut vault = st.vault.lock()?;
        let changed = vault.merge_payload(payload)?;
        vault.data.secrets.sync_revision = Some(revision);
        vault.data.settings.sync.last_sync_at = Some(now_ms());
        vault.save()?;
        changed
    };

    Ok(SyncResult {
        revision,
        uploaded: 0,
        downloaded: changed,
        message: format!("已从云端合并 {changed} 条变更（版本 {revision}）"),
    })
}

#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>) -> Result<SyncResult> {
    // 先拉取合并，再整体上传，保证两端一致
    let pulled = do_pull(&state).await?;
    let pushed = do_push(&state).await?;
    Ok(SyncResult {
        revision: pushed.revision,
        uploaded: pushed.uploaded,
        downloaded: pulled.downloaded,
        message: format!("{}；{}", pulled.message, pushed.message),
    })
}
