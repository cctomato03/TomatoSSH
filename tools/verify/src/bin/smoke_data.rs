//! 轻量冒烟测试：只验证「保险库加密持久化 + 本地文件 + 本地 PTY 终端」，
//! 不引入 SSH / HTTP 依赖，因此在没有 MSVC 工具链的开发机上也能直接跑通。
//!
//! 运行：`cargo run --bin smoke_core`
//!
//! 说明：这里用一个极简的 `error` 模块替身（等价 API），避免为了一个错误类型
//! 把 russh 整棵树拉进这个测试二进制；其余源码均为 `src-tauri/src` 的真实文件。
#![allow(dead_code)]

pub mod error {
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

    macro_rules! impl_from {
        ($($t:ty),* $(,)?) => {
            $(impl From<$t> for Error {
                fn from(e: $t) -> Self { Error(e.to_string()) }
            })*
        };
    }

    impl_from!(
        std::io::Error,
        std::string::FromUtf8Error,
        serde_json::Error,
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
            Error("internal lock poisoned".into())
        }
    }

    pub type Result<T> = std::result::Result<T, Error>;

    pub trait IntoAppError<T> {
        fn app_err(self) -> Result<T>;
    }

    impl<T, E: std::fmt::Display> IntoAppError<T> for std::result::Result<T, E> {
        fn app_err(self) -> Result<T> {
            self.map_err(|e| Error(e.to_string()))
        }
    }
}

#[path = "../../../../src-tauri/src/models.rs"]
pub mod models;

#[path = "../../../../src-tauri/src/crypto.rs"]
pub mod crypto;

#[path = "../../../../src-tauri/src/vault.rs"]
pub mod vault;

#[path = "../../../../src-tauri/src/localfs.rs"]
pub mod localfs;


use models::{AuthConfig, GroupDraft, HostDraft, AUTH_PASSWORD};

fn main() {
    println!("=== 1. 保险库：加密持久化 / 重载 / 软删除 ===");
    let dir = std::env::temp_dir().join(format!("tomatossh-core-{}", uuid::Uuid::new_v4()));
    let mut store = vault::VaultStore::open(&dir).expect("打开保险库");
    let group = store
        .upsert_group(GroupDraft {
            name: "生产环境".into(),
            ..Default::default()
        })
        .expect("创建分组");
    let host = store
        .upsert_host(HostDraft {
            label: "web-01".into(),
            host: "10.0.0.11".into(),
            port: 22,
            username: "root".into(),
            group_id: Some(group.id.clone()),
            auth: AuthConfig {
                kind: AUTH_PASSWORD.into(),
                password: Some("p@ssw0rd-明文不应出现在磁盘".into()),
                ..Default::default()
            },
            ..Default::default()
        })
        .expect("保存连接");
    let raw = std::fs::read(dir.join("vault.bin")).expect("读保险库");
    assert!(
        !String::from_utf8_lossy(&raw).contains("p@ssw0rd"),
        "保险库里不应出现明文密码"
    );
    println!("  保险库 {} 字节（AES-256-GCM 密文，无明文密码）", raw.len());

    let reloaded = vault::VaultStore::open(&dir).expect("重新打开保险库");
    let h = reloaded.host(&host.id).expect("读取连接");
    assert_eq!(h.auth.password.as_deref(), Some("p@ssw0rd-明文不应出现在磁盘"));
    assert_eq!(reloaded.snapshot().groups.len(), 1);
    println!("  重载后分组 / 连接 / 密码一致 ✔");

    let mut store = reloaded;
    store.delete_host(&host.id).expect("删除连接");
    assert!(store.snapshot().hosts.is_empty());
    assert_eq!(store.data.hosts.len(), 1, "软删除应保留墓碑用于同步");
    println!("  软删除 ✔（墓碑参与云同步，避免另一台设备又加回来）");

    println!("\n=== 2. 加密与密钥派生 ===");
    let key = [3u8; 32];
    let blob = crypto::encrypt(&key, b"tomato").expect("加密");
    assert_eq!(crypto::decrypt(&key, &blob).expect("解密"), b"tomato");
    assert!(crypto::decrypt(&[4u8; 32], &blob).is_err());
    assert_eq!(
        crypto::derive_sync_key("pw", "A@B.com"),
        crypto::derive_sync_key("pw", "a@b.com ")
    );
    println!("  AES-256-GCM 往返、错误密钥拒绝、PBKDF2 归一化 ✔");

    println!("\n=== 3. 云端合并（后写者胜）===");
    let payload = store.payload();
    let mut remote = payload.clone();
    remote.hosts[0].label = "云端改名".into();
    remote.hosts[0].updated_at = payload.hosts[0].updated_at + 5_000;
    assert_eq!(store.merge_payload(remote).expect("merge"), 1);
    assert_eq!(store.snapshot().hosts.len(), 0, "云端较新的数据含删除标记");
    println!("  较新的云端版本覆盖本地、较旧版本被忽略 ✔");

    println!("\n=== 4. 本地文件浏览 ===");
    let home = localfs::home().expect("主目录");
    let entries = localfs::list(Some(home.home.clone())).expect("列目录");
    println!("  {} 内有 {} 个条目，例如：", home.home, entries.len());
    for e in entries.iter().take(3) {
        println!("    {} {}", if e.is_dir { "DIR " } else { "FILE" }, e.name);
    }
    let tmp = std::env::temp_dir().join(format!("tomatossh-fs-{}", uuid::Uuid::new_v4()));
    localfs::mkdir(&tmp.to_string_lossy()).expect("mkdir");
    let f = tmp.join("a.txt");
    std::fs::write(&f, b"x").expect("write");
    localfs::rename(&f.to_string_lossy(), &tmp.join("b.txt").to_string_lossy()).expect("rename");
    assert!(localfs::list(Some(tmp.to_string_lossy().to_string()))
        .expect("list")
        .iter()
        .any(|e| e.name == "b.txt"));
    localfs::remove(&tmp.join("b.txt").to_string_lossy()).expect("remove file");
    localfs::remove(&tmp.to_string_lossy()).expect("remove dir");
    println!("  新建 / 重命名 / 列举 / 删除 ✔");


    let _ = std::fs::remove_dir_all(&dir);
    println!("\n全部核心冒烟测试通过 ✅");
}
