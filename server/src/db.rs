use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::ApiError;

pub struct Db {
    conn: Mutex<Connection>,
}

pub struct User {
    pub id: i64,
    pub password_hash: String,
}

pub struct VaultRow {
    pub revision: i64,
    pub blob: Option<String>,
    pub updated_at: i64,
}

impl Db {
    pub fn open(path: &str) -> Result<Self, ApiError> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| ApiError::internal(format!("无法创建数据目录: {e}")))?;
            }
        }
        let conn = Connection::open(path)
            .map_err(|e| ApiError::internal(format!("无法打开数据库: {e}")))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| ApiError::internal(format!("设置 WAL 失败: {e}")))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| ApiError::internal(format!("开启外键失败: {e}")))?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS users (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                email         TEXT NOT NULL UNIQUE,
                password_hash TEXT NOT NULL,
                created_at    INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS vaults (
                user_id     INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
                revision    INTEGER NOT NULL DEFAULT 0,
                blob        TEXT,
                updated_at  INTEGER NOT NULL DEFAULT 0,
                device_id   TEXT
            );
            CREATE TABLE IF NOT EXISTS devices (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                device_id  TEXT NOT NULL,
                name       TEXT,
                last_seen  INTEGER NOT NULL
            );
            "#,
        )
        .map_err(|e| ApiError::internal(format!("初始化数据表失败: {e}")))?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn create_user(&self, email: &str, password_hash: &str) -> Result<i64, ApiError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| ApiError::internal("数据库锁错误"))?;
        let now = now_ms();
        match conn.execute(
            "INSERT INTO users (email, password_hash, created_at) VALUES (?1, ?2, ?3)",
            params![email, password_hash, now],
        ) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                return Err(ApiError::conflict("该邮箱已注册"));
            }
            Err(e) => return Err(e.into()),
        }
        let id: i64 = conn
            .query_row("SELECT id FROM users WHERE email = ?1", params![email], |r| {
                r.get(0)
            })?;
        conn.execute(
            "INSERT INTO vaults (user_id, revision, blob, updated_at) VALUES (?1, 0, NULL, 0)",
            params![id],
        )?;
        Ok(id)
    }

    pub fn find_user(&self, email: &str) -> Result<Option<User>, ApiError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| ApiError::internal("数据库锁错误"))?;
        let row = conn
            .query_row(
                "SELECT id, password_hash FROM users WHERE email = ?1",
                params![email],
                |r| {
                    Ok(User {
                        id: r.get(0)?,
                        password_hash: r.get(1)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn get_vault(&self, user_id: i64) -> Result<Option<VaultRow>, ApiError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| ApiError::internal("数据库锁错误"))?;
        let row = conn
            .query_row(
                "SELECT revision, blob, updated_at FROM vaults WHERE user_id = ?1",
                params![user_id],
                |r| {
                    Ok(VaultRow {
                        revision: r.get(0)?,
                        blob: r.get(1)?,
                        updated_at: r.get(2)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// 保存保险库；`base_revision` 用于乐观并发控制
    pub fn put_vault(
        &self,
        user_id: i64,
        blob: &str,
        device_id: &str,
        base_revision: Option<i64>,
    ) -> Result<i64, ApiError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| ApiError::internal("数据库锁错误"))?;
        let current: Option<i64> = conn
            .query_row(
                "SELECT revision FROM vaults WHERE user_id = ?1",
                params![user_id],
                |r| r.get(0),
            )
            .optional()?;

        let current = current.unwrap_or(0);
        if let Some(base) = base_revision {
            if base != current {
                return Err(ApiError::conflict(format!(
                    "云端版本已更新（当前 {current}，你基于 {base}），请先拉取再上传"
                )));
            }
        }

        let next = current + 1;
        let now = now_ms();
        conn.execute(
            "INSERT INTO vaults (user_id, revision, blob, updated_at, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(user_id) DO UPDATE SET
                revision = excluded.revision,
                blob = excluded.blob,
                updated_at = excluded.updated_at,
                device_id = excluded.device_id",
            params![user_id, next, blob, now, device_id],
        )?;
        let _ = conn.execute(
            "INSERT INTO devices (user_id, device_id, name, last_seen) VALUES (?1, ?2, NULL, ?3)",
            params![user_id, device_id, now],
        );
        Ok(next)
    }
}

pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
