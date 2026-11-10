use serde::{Serialize, Serializer};

/// 统一错误类型：可直接作为 Tauri 命令的错误返回，前端拿到的是字符串。
#[derive(Debug)]
pub struct Error(pub String);

impl Error {
    pub fn msg(m: impl Into<String>) -> Self {
        Error(m.into())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

macro_rules! impl_from {
    ($($t:ty),* $(,)?) => {
        $(
            impl From<$t> for Error {
                fn from(e: $t) -> Self {
                    Error(e.to_string())
                }
            }
        )*
    };
}

impl_from!(
    std::io::Error,
    std::string::FromUtf8Error,
    serde_json::Error,
    anyhow::Error,
    russh::Error,
    russh_sftp::client::error::Error,
    reqwest::Error,
    base64::DecodeError,
    uuid::Error,
);

impl From<String> for Error {
    fn from(e: String) -> Self {
        Error(e)
    }
}

impl From<&str> for Error {
    fn from(e: &str) -> Self {
        Error(e.to_string())
    }
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        Error("内部锁已中毒（poisoned mutex）".into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// 把任意 Display 错误转成我们的错误类型（便于 `?` 用在第三方错误上）
pub trait IntoAppError<T> {
    fn app_err(self) -> Result<T>;
}

impl<T, E: std::fmt::Display> IntoAppError<T> for std::result::Result<T, E> {
    fn app_err(self) -> Result<T> {
        self.map_err(|e| Error(e.to_string()))
    }
}
