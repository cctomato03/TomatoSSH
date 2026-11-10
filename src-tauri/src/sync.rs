use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Serialize)]
struct AuthRequest<'a> {
    email: &'a str,
    password: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_name: Option<&'a str>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct AuthResponse {
    pub token: String,
    #[serde(default)]
    pub email: String,
}

#[derive(Deserialize, Debug)]
struct VaultResponse {
    #[serde(default)]
    revision: i64,
    #[serde(default)]
    blob: Option<String>,
    #[serde(default)]
    updated_at: Option<i64>,
}

#[derive(Serialize)]
struct PushRequest<'a> {
    blob: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_revision: Option<i64>,
    device_id: &'a str,
}

#[derive(Deserialize, Debug)]
struct PushResponse {
    #[serde(default)]
    revision: i64,
}

fn endpoint(server: &str, path: &str) -> String {
    let base = server.trim().trim_end_matches('/');
    format!("{base}{path}")
}

async fn read_error(resp: reqwest::Response) -> Error {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    let detail = if text.trim().is_empty() {
        String::new()
    } else {
        // 服务端统一返回 {"error": "..."} 或纯文本
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(v) => v
                .get("error")
                .and_then(|e| e.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| v.to_string()),
            Err(_) => text,
        }
    };
    if status.as_u16() == 401 {
        Error::msg("登录状态已失效，请重新登录同步账号")
    } else {
        Error::msg(format!("同步服务返回 {status}：{detail}"))
    }
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("TomatoSSH/0.1")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| Error::msg(format!("初始化 HTTP 客户端失败：{e}")))
}

pub async fn register(server: &str, email: &str, password: &str) -> Result<AuthResponse> {
    let resp = client()?
        .post(endpoint(server, "/api/v1/auth/register"))
        .json(&AuthRequest {
            email,
            password,
            device_name: Some("TomatoSSH Desktop"),
        })
        .send()
        .await
        .map_err(|e| Error::msg(format!("无法连接同步服务（{server}）：{e}")))?;

    if !resp.status().is_success() {
        return Err(read_error(resp).await);
    }
    resp.json::<AuthResponse>()
        .await
        .map_err(|e| Error::msg(format!("解析服务端响应失败：{e}")))
}

pub async fn login(server: &str, email: &str, password: &str) -> Result<AuthResponse> {
    let resp = client()?
        .post(endpoint(server, "/api/v1/auth/login"))
        .json(&AuthRequest {
            email,
            password,
            device_name: Some("TomatoSSH Desktop"),
        })
        .send()
        .await
        .map_err(|e| Error::msg(format!("无法连接同步服务（{server}）：{e}")))?;

    if !resp.status().is_success() {
        return Err(read_error(resp).await);
    }
    resp.json::<AuthResponse>()
        .await
        .map_err(|e| Error::msg(format!("解析服务端响应失败：{e}")))
}

/// 拉取云端保险库：返回 (revision, 加密后的 blob)
pub async fn pull(server: &str, token: &str) -> Result<Option<(i64, Vec<u8>)>> {
    let resp = client()?
        .get(endpoint(server, "/api/v1/sync/vault"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| Error::msg(format!("无法连接同步服务：{e}")))?;

    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(read_error(resp).await);
    }

    let body: VaultResponse = resp
        .json()
        .await
        .map_err(|e| Error::msg(format!("解析云端数据失败：{e}")))?;

    Ok(match body.blob {
        Some(b64) if !b64.is_empty() => {
            let raw = crate::crypto::b64_decode(&b64)?;
            Some((body.revision, raw))
        }
        _ => None,
    })
}

/// 上传云端保险库，返回新的 revision
pub async fn push(
    server: &str,
    token: &str,
    blob: &[u8],
    device_id: &str,
    base_revision: Option<i64>,
) -> Result<i64> {
    let encoded = crate::crypto::b64_encode(blob);
    let resp = client()?
        .put(endpoint(server, "/api/v1/sync/vault"))
        .bearer_auth(token)
        .json(&PushRequest {
            blob: &encoded,
            base_revision,
            device_id,
        })
        .send()
        .await
        .map_err(|e| Error::msg(format!("上传失败，无法连接同步服务：{e}")))?;

    if !resp.status().is_success() {
        return Err(read_error(resp).await);
    }
    let body: PushResponse = resp
        .json()
        .await
        .map_err(|e| Error::msg(format!("解析服务端响应失败：{e}")))?;
    Ok(body.revision)
}
