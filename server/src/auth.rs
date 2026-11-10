use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::db::now_ms;
use crate::error::ApiError;

type HmacSha256 = Hmac<Sha256>;

fn b64url(data: &[u8]) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    URL_SAFE_NO_PAD.encode(data)
}

fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    URL_SAFE_NO_PAD.decode(s).ok()
}

#[derive(Serialize, Deserialize, Debug)]
struct JwtHeader {
    alg: String,
    typ: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Claims {
    pub sub: i64,
    pub email: String,
    pub iat: i64,
    pub exp: i64,
}

pub fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::internal(format!("密码哈希失败: {e}")))
}

pub fn verify_password(hash: &str, password: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

fn sign(secret: &str, data: &str) -> Option<String> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(data.as_bytes());
    Some(b64url(&mac.finalize().into_bytes()))
}

pub fn issue_token(secret: &str, user_id: i64, email: &str, ttl_hours: i64) -> String {
    let now = now_ms() / 1000;
    let header = JwtHeader {
        alg: "HS256".into(),
        typ: "JWT".into(),
    };
    let claims = Claims {
        sub: user_id,
        email: email.to_string(),
        iat: now,
        exp: now + ttl_hours * 3600,
    };
    let h = b64url(&serde_json::to_vec(&header).unwrap_or_default());
    let p = b64url(&serde_json::to_vec(&claims).unwrap_or_default());
    let signing_input = format!("{h}.{p}");
    let sig = sign(secret, &signing_input).unwrap_or_default();
    format!("{signing_input}.{sig}")
}

pub fn verify_token(secret: &str, token: &str) -> Result<Claims, ApiError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(ApiError::unauthorized("token 格式不正确"));
    }
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let expected = sign(secret, &signing_input)
        .ok_or_else(|| ApiError::internal("签名计算失败"))?;
    // 常量时间比较，避免时序攻击
    if !constant_time_eq(expected.as_bytes(), parts[2].as_bytes()) {
        return Err(ApiError::unauthorized("token 签名无效"));
    }
    let payload = b64url_decode(parts[1]).ok_or_else(|| ApiError::unauthorized("token 载荷无效"))?;
    let claims: Claims = serde_json::from_slice(&payload)
        .map_err(|_| ApiError::unauthorized("token 载荷无法解析"))?;
    if claims.exp < now_ms() / 1000 {
        return Err(ApiError::unauthorized("登录已过期，请重新登录"));
    }
    Ok(claims)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

pub fn validate_credentials(email: &str, password: &str) -> Result<(), ApiError> {
    let email = email.trim();
    if email.len() < 3 || !email.contains('@') || email.len() > 254 {
        return Err(ApiError::bad_request("邮箱格式不正确"));
    }
    if password.chars().count() < 6 {
        return Err(ApiError::bad_request("密码至少 6 位"));
    }
    if password.len() > 512 {
        return Err(ApiError::bad_request("密码过长"));
    }
    Ok(())
}
