use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: String,
    pub db_path: String,
    pub jwt_secret: String,
    pub allow_register: bool,
    pub token_ttl_hours: i64,
    pub max_blob_bytes: usize,
}

impl Config {
    pub fn from_env() -> Self {
        let jwt_secret = env::var("JWT_SECRET").unwrap_or_default();
        let jwt_secret = if jwt_secret.trim().is_empty() {
            // 未配置时生成随机密钥（重启后旧 token 失效，仅适合本地试跑）
            use base64::{engine::general_purpose::STANDARD, Engine};
            let mut buf = [0u8; 48];
            use rand::RngCore;
            rand::thread_rng().fill_bytes(&mut buf);
            tracing::warn!("未设置 JWT_SECRET，已生成临时密钥（重启后需要重新登录）");
            STANDARD.encode(buf)
        } else {
            jwt_secret
        };

        Self {
            bind: env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8787".to_string()),
            db_path: env::var("DATABASE_PATH").unwrap_or_else(|_| "data/tomatossh.db".to_string()),
            jwt_secret,
            allow_register: env::var("ALLOW_REGISTER")
                .map(|v| !matches!(v.to_lowercase().as_str(), "0" | "false" | "no"))
                .unwrap_or(true),
            token_ttl_hours: env::var("TOKEN_TTL_HOURS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(24 * 30),
            max_blob_bytes: env::var("MAX_BLOB_BYTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(16 * 1024 * 1024),
        }
    }
}
