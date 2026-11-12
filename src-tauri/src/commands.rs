use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::error::{Error, Result};
use crate::local::LocalManager;
use crate::localfs;
use crate::models::*;
use crate::queue::{EnqueueJob, TransferJob, TransferQueue};
use crate::forward::{ActiveForward, ForwardManager};
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
    pub forwards: Arc<ForwardManager>,
    pub queue: Arc<TransferQueue>,
}

impl AppState {
    pub fn new(
        vault: Arc<Mutex<VaultStore>>,
        ssh: Arc<SshManager>,
        sftp: Arc<SftpManager>,
        local: Arc<LocalManager>,
        serial: Arc<SerialManager>,
        forwards: Arc<ForwardManager>,
        queue: Arc<TransferQueue>,
    ) -> Self {
        Self {
            vault,
            ssh,
            sftp,
            local,
            serial,
            forwards,
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

    let jump = jump_host_of(&state, &host)?;
    let jump_for_forward = jump.clone();
    let opened = state
        .ssh
        .open_terminal(app, session_id, host.clone(), cols, rows, jump)
        .await?;

    // 打开终端的连接如果有 autoStart 的转发规则，顺手启动
    for rule in host.forwards.iter().filter(|r| r.auto_start) {
        let _ = state
            .forwards
            .start(
                &host,
                jump_for_forward.clone(),
                &rule.bind_host,
                rule.bind_port,
                &rule.dest_host,
                rule.dest_port,
            )
            .await;
    }

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
    write_bytes(&state, &session_id, data.as_bytes()).await
}

/// 按原始字节写终端（HEX 发送用）
#[tauri::command]
pub async fn term_write_b64(
    state: State<'_, AppState>,
    session_id: String,
    data_b64: String,
) -> Result<()> {
    let bytes = crate::crypto::b64_decode(&data_b64)?;
    write_bytes(&state, &session_id, &bytes).await
}

async fn write_bytes(state: &AppState, session_id: &str, data: &[u8]) -> Result<()> {
    if state.ssh.has_session(session_id) {
        state.ssh.write(session_id, data).await
    } else if state.local.has(session_id) {
        state.local.write(session_id, data)
    } else if state.serial.has(session_id) {
        state.serial.write(session_id, data)
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
    let jump = jump_host_of(&state, &host)?;
    let opened = state.sftp.open(&session_id, &host, jump.as_ref()).await?;
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

// ---------------------------------------------------------------- 快捷指令

#[tauri::command]
pub fn snippet_save(state: State<'_, AppState>, draft: Snippet) -> Result<Snippet> {
    let mut vault = state.vault.lock()?;
    vault.upsert_snippet(draft)
}

#[tauri::command]
pub fn snippet_delete(state: State<'_, AppState>, id: String) -> Result<()> {
    let mut vault = state.vault.lock()?;
    vault.delete_snippet(&id)
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
    let mut profile = {
        let vault = state.vault.lock()?;
        vault.serial(&profile_id)?
    };
    let opened = state.serial.open(app, session_id, &profile)?;
    // 记住这次真正用上的端口（USB 重新枚举后能自动找回）
    if profile.last_port_name.as_deref() != Some(opened.port_name.as_str()) {
        profile.last_port_name = Some(opened.port_name.clone());
        if let Ok(mut vault) = state.vault.lock() {
            let _ = vault.upsert_serial(profile);
        }
    }
    Ok(opened.banner)
}

/// 手动设置 DTR / RTS 电平
#[tauri::command]
pub fn serial_set_line(
    state: State<'_, AppState>,
    session_id: String,
    line: String,
    value: bool,
) -> Result<()> {
    state.serial.set_line(&session_id, &line, value)
}

/// 读取当前控制线状态（DTR/RTS/CTS/CD）
#[tauri::command]
pub fn serial_get_lines(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<crate::serial::SerialLines> {
    state.serial.get_lines(&session_id)
}

/// 复位脉冲（DTR/RTS 拉低再释放），调试开发板常用
#[tauri::command]
pub fn serial_pulse_reset(state: State<'_, AppState>, session_id: String) -> Result<()> {
    state.serial.pulse_reset(&session_id)
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


// ---------------------------------------------------------------- 串口日志 / 波特率探测

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStatus {
    pub path: String,
    pub bytes: u64,
}

#[tauri::command]
pub fn serial_start_log(state: State<'_, AppState>, session_id: String, path: String) -> Result<()> {
    state.serial.start_log(&session_id, &path)
}

#[tauri::command]
pub fn serial_stop_log(state: State<'_, AppState>, session_id: String) -> Result<Option<LogStatus>> {
    Ok(state
        .serial
        .stop_log(&session_id)?
        .map(|(path, bytes)| LogStatus { path, bytes }))
}

#[tauri::command]
pub fn serial_log_status(state: State<'_, AppState>, session_id: String) -> Result<Option<LogStatus>> {
    Ok(state
        .serial
        .log_status(&session_id)
        .map(|(path, bytes)| LogStatus { path, bytes }))
}

#[tauri::command]
pub fn serial_probe_baud(
    state: State<'_, AppState>,
    port_name: String,
    candidates: Option<Vec<u32>>,
    per_try_ms: Option<u64>,
) -> Result<crate::serial::BaudProbe> {
    let list = candidates.unwrap_or_else(|| {
        vec![
            9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600, 4800, 2400, 1200,
        ]
    });
    let per_try = std::time::Duration::from_millis(per_try_ms.unwrap_or(400).clamp(120, 3000));
    state.serial.probe_baud(&port_name, &list, per_try)
}

// ---------------------------------------------------------------- 端口转发（本地 -L）

#[tauri::command]
pub async fn forward_start(
    state: State<'_, AppState>,
    host_id: String,
    bind_host: String,
    bind_port: u16,
    dest_host: String,
    dest_port: u16,
) -> Result<ActiveForward> {
    let host = {
        let vault = state.vault.lock()?;
        vault.host(&host_id)?
    };
    let jump = jump_host_of(&state, &host)?;
    let info = state
        .forwards
        .start(&host, jump, &bind_host, bind_port, &dest_host, dest_port)
        .await?;
    // 记下实际用的端口（bind_port 为 0 时由系统分配）
    let mut vault = state.vault.lock()?;
    if let Ok(h) = vault.host(&host_id) {
        let mut updated = HostDraft {
            id: Some(h.id.clone()),
            group_id: h.group_id.clone(),
            label: h.label.clone(),
            host: h.host.clone(),
            port: h.port,
            username: h.username.clone(),
            auth: h.auth.clone(),
            color: h.color.clone(),
            tags: h.tags.clone(),
            notes: h.notes.clone(),
            keepalive_secs: h.keepalive_secs,
            jump_host_id: h.jump_host_id.clone(),
            forwards: h.forwards.clone(),
        };
        let mut touched = false;
        for rule in updated.forwards.iter_mut() {
            if rule.dest_host == info.dest_host
                && rule.dest_port == info.dest_port
                && (rule.bind_port == 0 || rule.bind_port == info.bind_port)
            {
                rule.bind_port = info.bind_port;
                touched = true;
                break;
            }
        }
        if touched {
            let _ = vault.upsert_host(updated);
        }
    }
    Ok(info)
}

#[tauri::command]
pub fn forward_list(state: State<'_, AppState>) -> Result<Vec<ActiveForward>> {
    Ok(state.forwards.list())
}

#[tauri::command]
pub fn forward_stop(state: State<'_, AppState>, id: String) -> Result<()> {
    state.forwards.stop(&id)
}

/// 启动某主机上配置的所有 autoStart 规则
#[tauri::command]
pub async fn forward_start_auto(state: State<'_, AppState>, host_id: String) -> Result<usize> {
    let host = {
        let vault = state.vault.lock()?;
        vault.host(&host_id)?
    };
    let jump = jump_host_of(&state, &host)?;
    let rules: Vec<_> = host.forwards.iter().filter(|r| r.auto_start).cloned().collect();
    let mut started = 0usize;
    for rule in rules {
        if state
            .forwards
            .start(&host, jump.clone(), &rule.bind_host, rule.bind_port, &rule.dest_host, rule.dest_port)
            .await
            .is_ok()
        {
            started += 1;
        }
    }
    Ok(started)
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

/// 取出跳板机配置（没有则 None）
fn jump_host_of(state: &AppState, host: &Host) -> Result<Option<Host>> {
    let Some(jump_id) = host.jump_host_id.as_deref() else {
        return Ok(None);
    };
    if jump_id.is_empty() || jump_id == host.id {
        return Ok(None);
    }
    let vault = state.vault.lock()?;
    Ok(vault.host(jump_id).ok())
}

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
