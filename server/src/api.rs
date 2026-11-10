use std::sync::Arc;

use axum::http::{header::AUTHORIZATION, HeaderMap};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::auth::{self, Claims};
use crate::config::Config;
use crate::db::Db;
use crate::error::ApiError;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub cfg: Arc<Config>,
}

#[derive(Deserialize)]
pub struct AuthRequest {
    pub email: String,
    pub password: String,
    #[serde(default, alias = "deviceName")]
    pub device_name: Option<String>,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub email: String,
    pub expires_in: i64,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
    pub allow_register: bool,
}

pub async fn health(State(st): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "tomatossh-sync",
        version: env!("CARGO_PKG_VERSION"),
        allow_register: st.cfg.allow_register,
    })
}

pub async fn register(
    State(st): State<AppState>,
    Json(req): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    if !st.cfg.allow_register {
        return Err(ApiError::forbidden("本服务已关闭注册，请联系管理员"));
    }
    auth::validate_credentials(&req.email, &req.password)?;
    let email = auth::normalize_email(&req.email);
    let password = req.password.clone();

    let hash = tokio::task::spawn_blocking(move || auth::hash_password(&password))
        .await
        .map_err(|e| ApiError::internal(format!("哈希任务失败: {e}")))??;

    let db = st.db.clone();
    let email_for_db = email.clone();
    let user_id = tokio::task::spawn_blocking(move || db.create_user(&email_for_db, &hash))
        .await
        .map_err(|e| ApiError::internal(format!("注册任务失败: {e}")))??;

    let token = auth::issue_token(&st.cfg.jwt_secret, user_id, &email, st.cfg.token_ttl_hours);
    tracing::info!(user_id, %email, device = ?req.device_name, "新用户注册");
    Ok(Json(AuthResponse {
        token,
        email,
        expires_in: st.cfg.token_ttl_hours * 3600,
    }))
}

pub async fn login(
    State(st): State<AppState>,
    Json(req): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    let email = auth::normalize_email(&req.email);
    let password = req.password.clone();
    let db = st.db.clone();
    let email_for_db = email.clone();

    let user = tokio::task::spawn_blocking(move || db.find_user(&email_for_db))
        .await
        .map_err(|e| ApiError::internal(format!("查询任务失败: {e}")))??;

    let Some(user) = user else {
        return Err(ApiError::unauthorized("邮箱或密码不正确"));
    };

    let hash = user.password_hash.clone();
    let ok = tokio::task::spawn_blocking(move || auth::verify_password(&hash, &password))
        .await
        .map_err(|e| ApiError::internal(format!("校验任务失败: {e}")))?;
    if !ok {
        return Err(ApiError::unauthorized("邮箱或密码不正确"));
    }

    let token = auth::issue_token(&st.cfg.jwt_secret, user.id, &email, st.cfg.token_ttl_hours);
    tracing::info!(user_id = user.id, %email, device = ?req.device_name, "用户登录");
    Ok(Json(AuthResponse {
        token,
        email,
        expires_in: st.cfg.token_ttl_hours * 3600,
    }))
}

fn claims_from(headers: &HeaderMap, cfg: &Config) -> Result<Claims, ApiError> {
    let raw = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("缺少 Authorization 请求头"))?;
    let token = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))
        .ok_or_else(|| ApiError::unauthorized("Authorization 需为 Bearer <token>"))?;
    let claims = auth::verify_token(&cfg.jwt_secret, token.trim())?;
    Ok(claims)
}

#[derive(Serialize)]
pub struct VaultResponse {
    pub revision: i64,
    pub blob: Option<String>,
    pub updated_at: i64,
}

pub async fn get_vault(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<VaultResponse>, ApiError> {
    let claims = claims_from(&headers, &st.cfg)?;
    let db = st.db.clone();
    let row = tokio::task::spawn_blocking(move || db.get_vault(claims.sub))
        .await
        .map_err(|e| ApiError::internal(format!("查询任务失败: {e}")))??;

    match row {
        Some(r) => Ok(Json(VaultResponse {
            revision: r.revision,
            blob: r.blob,
            updated_at: r.updated_at,
        })),
        None => Err(ApiError::not_found("云端还没有数据")),
    }
}

#[derive(Deserialize)]
pub struct PushRequest {
    pub blob: String,
    #[serde(default, alias = "baseRevision")]
    pub base_revision: Option<i64>,
    #[serde(default, alias = "deviceId")]
    pub device_id: String,
}

#[derive(Serialize)]
pub struct PushResponse {
    pub revision: i64,
    pub updated_at: i64,
}

pub async fn put_vault(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PushRequest>,
) -> Result<Json<PushResponse>, ApiError> {
    let claims = claims_from(&headers, &st.cfg)?;
    if req.blob.len() > st.cfg.max_blob_bytes {
        return Err(ApiError::bad_request(format!(
            "保险库超过大小限制（{} 字节）",
            st.cfg.max_blob_bytes
        )));
    }

    let db = st.db.clone();
    let device_id = if req.device_id.is_empty() {
        "unknown".to_string()
    } else {
        req.device_id.clone()
    };
    let blob = req.blob.clone();
    let base = req.base_revision;
    let user_id = claims.sub;

    let revision = tokio::task::spawn_blocking(move || {
        db.put_vault(user_id, &blob, &device_id, base)
    })
    .await
    .map_err(|e| ApiError::internal(format!("写入任务失败: {e}")))??;

    Ok(Json(PushResponse {
        revision,
        updated_at: crate::db::now_ms(),
    }))
}
