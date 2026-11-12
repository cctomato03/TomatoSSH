mod commands;
mod crypto;
mod error;
mod forward;
mod local;
mod queue;
mod localfs;
mod models;
mod serial;
mod sftp;
mod ssh;
mod sync;
mod vault;

use tauri::Manager;

use commands::AppState;
use vault::VaultStore;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app
                .path()
                .app_config_dir()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            let vault = std::sync::Arc::new(std::sync::Mutex::new(
                VaultStore::open(&dir).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?,
            ));
            let sftp = std::sync::Arc::new(crate::sftp::SftpManager::default());
            // 后台传输队列：与 UI 窗口解耦，关掉文件管理窗口也会继续传
            let queue = std::sync::Arc::new(crate::queue::TransferQueue::new(
                app.handle().clone(),
                sftp.clone(),
                vault.clone(),
                dir.join("transfer-queue.json"),
            ));
            queue.start();
            app.manage(AppState::new(
                vault,
                std::sync::Arc::new(crate::ssh::SshManager::default()),
                sftp,
                std::sync::Arc::new(crate::local::LocalManager::default()),
                std::sync::Arc::new(crate::serial::SerialManager::default()),
                std::sync::Arc::new(crate::forward::ForwardManager::default()),
                queue,
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_load,
            commands::host_save,
            commands::host_delete,
            commands::host_clear_fingerprint,
            commands::group_save,
            commands::group_delete,
            commands::settings_save,
            commands::ssh_target,
            commands::ssh_open,
            commands::local_open,
            commands::term_write,
            commands::term_write_b64,
            commands::term_resize,
            commands::term_close,
            commands::sftp_open,
            commands::sftp_close,
            commands::sftp_home,
            commands::sftp_list,
            commands::sftp_mkdir,
            commands::sftp_remove,
            commands::sftp_rename,
            commands::sftp_download,
            commands::sftp_upload,
            commands::sftp_copy_remote,
            commands::local_list,
            commands::local_home,
            commands::local_mkdir,
            commands::local_remove,
            commands::local_rename,
            commands::local_copy,
            commands::open_file_manager,
            commands::enqueue_transfers,
            commands::list_transfers,
            commands::cancel_transfer,
            commands::retry_transfer,
            commands::clear_finished_transfers,
            commands::app_version,
            commands::snippet_save,
            commands::snippet_delete,
            commands::serial_list_ports,
            commands::serial_save,
            commands::serial_delete,
            commands::serial_open,
            commands::serial_set_line,
            commands::serial_get_lines,
            commands::serial_pulse_reset,
            commands::serial_start_log,
            commands::serial_stop_log,
            commands::serial_log_status,
            commands::serial_probe_baud,
            commands::forward_start,
            commands::forward_list,
            commands::forward_stop,
            commands::forward_start_auto,
            commands::sync_state,
            commands::sync_login,
            commands::sync_register,
            commands::sync_logout,
            commands::sync_push,
            commands::sync_pull,
            commands::sync_now,
        ])
        .run(tauri::generate_context!())
        .expect("TomatoSSH 启动失败");
}
