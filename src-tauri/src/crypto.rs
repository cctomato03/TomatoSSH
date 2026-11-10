use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;
use sha2::Sha256;

use crate::error::{Error, Result};

const MAGIC: &[u8; 5] = b"TSSH1";
const NONCE_LEN: usize = 12;

pub fn random_bytes(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut buf);
    buf
}

/// AES-256-GCM 加密：magic(5) || nonce(12) || ciphertext
pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| Error::msg(e.to_string()))?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| Error::msg(format!("加密失败: {e}")))?;

    let mut out = Vec::with_capacity(MAGIC.len() + NONCE_LEN + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn decrypt(key: &[u8; 32], blob: &[u8]) -> Result<Vec<u8>> {
    if blob.len() < MAGIC.len() + NONCE_LEN {
        return Err(Error::msg("保险库文件已损坏（长度不足）"));
    }
    if &blob[..MAGIC.len()] != MAGIC {
        return Err(Error::msg("不是 TomatoSSH 保险库文件（魔数不正确）"));
    }
    let nonce_bytes = &blob[MAGIC.len()..MAGIC.len() + NONCE_LEN];
    let ciphertext = &blob[MAGIC.len() + NONCE_LEN..];
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| Error::msg(e.to_string()))?;
    cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|_| Error::msg("解密失败：密钥不匹配或数据被篡改"))
}

/// 用账号密码派生同步加密密钥（服务端永远拿不到明文与密钥）
pub fn derive_sync_key(password: &str, email: &str) -> [u8; 32] {
    let salt = format!("tomatossh-vault|{}", email.trim().to_lowercase());
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), 120_000, &mut key);
    key
}

pub fn key_to_b64(key: &[u8; 32]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(key)
}

pub fn key_from_b64(s: &str) -> Result<[u8; 32]> {
    use base64::Engine;
    let raw = base64::engine::general_purpose::STANDARD.decode(s)?;
    if raw.len() != 32 {
        return Err(Error::msg("同步密钥长度不正确"));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&raw);
    Ok(key)
}

pub fn b64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

pub fn b64_decode(s: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.decode(s)?)
}
