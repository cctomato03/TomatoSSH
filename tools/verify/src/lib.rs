//! 编译校验入口：直接复用 `src-tauri/src` 下的真实源码，
//! 只把 tauri 运行时替换成 `tools/shim-tauri` 桩，用来在没有
//! MSVC / Tauri 构建环境的机器上验证后端代码能否编译通过。
#![allow(dead_code)]

#[path = "../../../src-tauri/src/error.rs"]
pub mod error;

#[path = "../../../src-tauri/src/models.rs"]
pub mod models;

#[path = "../../../src-tauri/src/crypto.rs"]
pub mod crypto;

#[path = "../../../src-tauri/src/vault.rs"]
pub mod vault;

#[path = "../../../src-tauri/src/ssh.rs"]
pub mod ssh;

#[path = "../../../src-tauri/src/serial.rs"]
pub mod serial;

#[path = "../../../src-tauri/src/queue.rs"]
pub mod queue;

#[path = "../../../src-tauri/src/sftp.rs"]
pub mod sftp;

#[path = "../../../src-tauri/src/local.rs"]
pub mod local;

#[path = "../../../src-tauri/src/localfs.rs"]
pub mod localfs;

#[path = "../../../src-tauri/src/sync.rs"]
pub mod sync;

#[path = "../../../src-tauri/src/commands.rs"]
pub mod commands;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app
                .path()
                .app_config_dir()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            let vault = std::sync::Arc::new(std::sync::Mutex::new(
                vault::VaultStore::open(&dir)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?,
            ));
            let sftp = std::sync::Arc::new(sftp::SftpManager::default());
            let queue = std::sync::Arc::new(queue::TransferQueue::new(
                tauri::AppHandle::new(),
                sftp.clone(),
                vault.clone(),
                dir.join("transfer-queue.json"),
            ));
            app.manage(commands::AppState::new(
                vault,
                std::sync::Arc::new(ssh::SshManager::default()),
                sftp,
                std::sync::Arc::new(local::LocalManager::default()),
                std::sync::Arc::new(serial::SerialManager::default()),
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
        .expect("verify ok");
}
