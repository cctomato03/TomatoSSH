use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use tauri::{AppHandle, Emitter};

use crate::crypto::b64_encode;
use crate::error::{Error, Result};

struct LocalTerm {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Default, Clone)]
pub struct LocalManager {
    terms: Arc<Mutex<HashMap<String, LocalTerm>>>,
}

/// 选择一个可用的本地 shell
pub fn default_shell() -> (String, Vec<String>) {
    #[cfg(windows)]
    {
        for candidate in [
            "pwsh.exe",
            "powershell.exe",
        ] {
            if which(candidate) {
                return (candidate.to_string(), vec!["-NoLogo".to_string()]);
            }
        }
        let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
        (comspec, Vec::new())
    }
    #[cfg(not(windows))]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let args = if shell.ends_with("fish") {
            vec![]
        } else {
            vec!["-l".to_string()]
        };
        (shell, args)
    }
}

#[cfg(windows)]
fn which(cmd: &str) -> bool {
    std::process::Command::new("where")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

impl LocalManager {
    pub fn open(
        &self,
        app: AppHandle,
        session_id: String,
        cols: u16,
        rows: u16,
        cwd: Option<String>,
    ) -> Result<String> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: rows.max(5),
                cols: cols.max(20),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| Error::msg(format!("创建本地终端失败：{e}")))?;

        let (shell, args) = default_shell();
        let mut cmd = CommandBuilder::new(&shell);
        for a in &args {
            cmd.arg(a);
        }
        if let Some(dir) = cwd.as_ref().filter(|d| !d.trim().is_empty()) {
            cmd.cwd(dir);
        } else if let Some(home) = dirs::home_dir() {
            cmd.cwd(home);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| Error::msg(format!("启动 shell 失败（{shell}）：{e}")))?;
        let killer = child.clone_killer();
        // 父进程不再需要 slave 端
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| Error::msg(format!("读取终端输出失败：{e}")))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| Error::msg(format!("写入终端失败：{e}")))?;

        self.terms.lock()?.insert(
            session_id.clone(),
            LocalTerm {
                master: pair.master,
                writer,
                killer,
            },
        );

        // 后台线程：把 PTY 输出推给前端
        let sid = session_id.clone();
        let terms = self.terms.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = app.emit(&format!("term-data:{sid}"), b64_encode(&buf[..n]));
                    }
                    Err(_) => break,
                }
            }
            // 子进程结束后再探一次，避免误判
            let _ = child.try_wait();
            if let Ok(mut guard) = terms.lock() {
                guard.remove(&sid);
            }
            let _ = app.emit(
                &format!("term-exit:{sid}"),
                serde_json::json!({ "code": null, "message": "本地 shell 已退出" }),
            );
        });

        Ok(format!("\x1b[38;5;244m—— 本地终端（{shell}）——\x1b[0m"))
    }

    pub fn write(&self, session_id: &str, data: &[u8]) -> Result<()> {
        let mut guard = self.terms.lock()?;
        let term = guard
            .get_mut(session_id)
            .ok_or_else(|| Error::msg("本地终端不存在或已关闭"))?;
        term.writer
            .write_all(data)
            .map_err(|e| Error::msg(format!("写入终端失败：{e}")))?;
        term.writer.flush().ok();
        Ok(())
    }

    pub fn resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<()> {
        let guard = self.terms.lock()?;
        if let Some(term) = guard.get(session_id) {
            let _ = term.master.resize(PtySize {
                rows: rows.max(5),
                cols: cols.max(20),
                pixel_width: 0,
                pixel_height: 0,
            });
        }
        Ok(())
    }

    pub fn close(&self, session_id: &str) {
        if let Ok(mut guard) = self.terms.lock() {
            if let Some(mut term) = guard.remove(session_id) {
                let _ = term.killer.kill();
            }
        }
    }

    pub fn has(&self, session_id: &str) -> bool {
        self.terms
            .lock()
            .map(|g| g.contains_key(session_id))
            .unwrap_or(false)
    }
}
