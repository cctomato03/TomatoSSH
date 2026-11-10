use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::models::{FileEntry, LocalHome};

fn to_string_path(p: &std::path::Path) -> String {
    p.to_string_lossy().to_string()
}

pub fn home() -> Result<LocalHome> {
    let home = dirs::home_dir().ok_or_else(|| Error::msg("无法定位用户主目录"))?;
    let downloads = dirs::download_dir().unwrap_or_else(|| home.join("Downloads"));
    let desktop = dirs::desktop_dir().unwrap_or_else(|| home.join("Desktop"));
    Ok(LocalHome {
        home: to_string_path(&home),
        downloads: to_string_path(&downloads),
        desktop: to_string_path(&desktop),
        is_windows: cfg!(windows),
    })
}

pub fn resolve(path: Option<String>) -> Result<PathBuf> {
    match path {
        Some(p) if !p.trim().is_empty() => Ok(PathBuf::from(p)),
        _ => dirs::home_dir().ok_or_else(|| Error::msg("无法定位用户主目录")),
    }
}

pub fn list(path: Option<String>) -> Result<Vec<FileEntry>> {
    let dir = resolve(path)?;
    if !dir.is_dir() {
        return Err(Error::msg(format!("不是有效目录：{}", to_string_path(&dir))));
    }

    let mut items = Vec::new();
    let read = std::fs::read_dir(&dir)
        .map_err(|e| Error::msg(format!("读取目录失败（{}）：{e}", to_string_path(&dir))))?;

    for entry in read.flatten() {
        let path = entry.path();
        let symlink_meta = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_symlink = symlink_meta.file_type().is_symlink();
        let meta = if is_symlink {
            std::fs::metadata(&path).unwrap_or(symlink_meta.clone())
        } else {
            symlink_meta.clone()
        };
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            meta.permissions().mode()
        };
        #[cfg(not(unix))]
        let mode = if meta.permissions().readonly() { 0o444 } else { 0o644 };

        items.push(FileEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: to_string_path(&path),
            is_dir: meta.is_dir(),
            is_symlink,
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified,
            mode,
        });
    }

    items.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(items)
}

pub fn mkdir(path: &str) -> Result<()> {
    std::fs::create_dir(path).map_err(|e| Error::msg(format!("创建目录失败：{e}")))
}

pub fn remove(path: &str) -> Result<()> {
    let p = PathBuf::from(path);
    let meta = std::fs::symlink_metadata(&p).map_err(|e| Error::msg(format!("无法访问：{e}")))?;
    let res = if meta.is_dir() {
        std::fs::remove_dir(&p)
    } else {
        std::fs::remove_file(&p)
    };
    res.map_err(|e| Error::msg(format!("删除失败：{e}")))
}

pub fn rename(from: &str, to: &str) -> Result<()> {
    std::fs::rename(from, to).map_err(|e| Error::msg(format!("重命名失败：{e}")))
}
