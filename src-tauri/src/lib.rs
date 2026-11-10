mod commands;
mod crypto;
mod error;
mod local;
mod localfs;
mod models;
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
            let vault = VaultStore::open(&dir)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            app.manage(AppState::new(vault));
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
