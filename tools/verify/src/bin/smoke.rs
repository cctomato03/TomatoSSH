//! 后端冒烟测试：不依赖 Tauri 运行时，直接验证保险库加密持久化、
//! 本地文件浏览、本地 PTY 终端、SSH 连接错误处理与云同步合并逻辑。
//!
//! 运行：`cargo run --bin smoke`

use std::time::Duration;

use tauri::AppHandle;
use tomatossh_verify::{
    crypto,
    local::{default_shell, LocalManager},
    localfs,
    models::{AuthConfig, GroupDraft, Host, HostDraft, AUTH_PASSWORD},
    ssh, vault,
};

fn step(name: &str) {
    println!("\n=== {name} ===");
}

fn main() {
    step("1. 保险库：加密持久化 + 重新加载");
    let dir = std::env::temp_dir().join(format!("tomatossh-smoke-{}", uuid::Uuid::new_v4()));
    let mut store = vault::VaultStore::open(&dir).expect("打开保险库");
    let group = store
        .upsert_group(GroupDraft {
            name: "测试分组".into(),
            ..Default::default()
        })
        .expect("创建分组");
    let host = store
        .upsert_host(HostDraft {
            label: "示例机器".into(),
            host: "127.0.0.1".into(),
            port: 2222,
            username: "root".into(),
            group_id: Some(group.id.clone()),
            auth: AuthConfig {
                kind: AUTH_PASSWORD.into(),
                password: Some("p@ssw0rd".into()),
                ..Default::default()
            },
            ..Default::default()
        })
        .expect("保存连接");
    assert_eq!(store.snapshot().hosts.len(), 1);

    let raw_vault = std::fs::read(dir.join("vault.bin")).expect("读取保险库文件");
    assert!(
        !String::from_utf8_lossy(&raw_vault).contains("p@ssw0rd"),
        "保险库文件里不应出现明文密码"
    );
    println!("保险库文件 {} 字节，已确认不含明文密码", raw_vault.len());

    let reloaded = vault::VaultStore::open(&dir).expect("重新打开保险库");
    let loaded_host = reloaded.host(&host.id).expect("读取连接");
    assert_eq!(loaded_host.auth.password.as_deref(), Some("p@ssw0rd"));
    assert_eq!(reloaded.data.groups.len(), 1);
    println!("重新加载后连接与分组、密码均正确还原");

    step("2. 加密：AES-256-GCM 往返 + 错误密钥必须失败");
    let key = [7u8; 32];
    let blob = crypto::encrypt(&key, b"hello tomatossh").expect("加密");
    assert_eq!(crypto::decrypt(&key, &blob).expect("解密"), b"hello tomatossh");
    assert!(crypto::decrypt(&[9u8; 32], &blob).is_err(), "错误密钥解密必须失败");
    let derived = crypto::derive_sync_key("super-secret", "a@b.com");
    assert_eq!(derived, crypto::derive_sync_key("super-secret", "A@B.com "));
    println!("加解密与 PBKDF2 密钥派生正常（同一账号大小写/空格归一化一致）");

    step("3. 云同步：合并逻辑（后写者胜 + 墓碑删除）");
    let payload = store.payload();
    assert_eq!(payload.hosts.len(), 1);
    let mut remote = payload.clone();
    remote.hosts[0].label = "云端改名".into();
    remote.hosts[0].updated_at = payload.hosts[0].updated_at + 10_000;
    let changed = store.merge_payload(remote).expect("合并云端数据");
    assert_eq!(changed, 1);
    assert_eq!(store.snapshot().hosts[0].label, "云端改名");
    println!("云端较新版本已覆盖本地，变更数 = {changed}");

    let stale = payload.clone();
    let changed = store.merge_payload(stale).expect("合并不应覆盖");
    assert_eq!(changed, 0, "较旧版本不应覆盖较新版本");
    println!("较旧的云端数据被正确忽略");

    step("4. 本地文件系统");
    let home = localfs::home().expect("获取主目录");
    println!("主目录: {}", home.home);
    let entries = localfs::list(Some(home.home.clone())).expect("列出目录");
    println!("本地目录项 {} 个，示例:", entries.len());
    for e in entries.iter().take(5) {
        println!(
            "  {} {} {}",
            if e.is_dir { "DIR " } else { "FILE" },
            e.name,
            e.size
        );
    }
    let tmp_dir = std::env::temp_dir().join(format!("tomatossh-fs-{}", uuid::Uuid::new_v4()));
    localfs::mkdir(&tmp_dir.to_string_lossy()).expect("新建目录");
    let f = tmp_dir.join("note.txt");
    std::fs::write(&f, b"abc").expect("写文件");
    let listed = localfs::list(Some(tmp_dir.to_string_lossy().to_string())).expect("列目录");
    assert!(listed.iter().any(|e| e.name == "note.txt"));
    localfs::remove(&f.to_string_lossy()).expect("删除文件");
    localfs::remove(&tmp_dir.to_string_lossy()).expect("删除目录");
    println!("本地目录创建/列举/删除均正常");

    step("5. 本地终端（portable-pty）");
    let app = AppHandle::new();
    let mgr = LocalManager::default();
    let (shell, args) = default_shell();
    println!("使用 shell: {shell} {args:?}");
    let banner = mgr
        .open(app.clone(), "smoke-term".into(), 120, 30, Some(home.home.clone()))
        .expect("打开本地终端");
    println!("终端输出: {banner}");
    std::thread::sleep(Duration::from_millis(1200));
    mgr.write("smoke-term", b"echo TOMATO_PTY_OK\r\n").expect("写入终端");
    std::thread::sleep(Duration::from_millis(1800));

    let events = app.take_events();
    let mut output = String::new();
    for (name, payload) in events {
        if name.starts_with("term-data:") {
            // 事件载荷是 base64 编码的输出
            if let Ok(bytes) = crypto::b64_decode(payload.trim_matches('"')) {
                output.push_str(&String::from_utf8_lossy(&bytes));
            }
        }
    }
    mgr.close("smoke-term");
    assert!(
        output.contains("TOMATO_PTY_OK"),
        "本地终端未返回预期输出，实际输出: {output}"
    );
    println!("本地终端已回显 TOMATO_PTY_OK（{} 字节输出）", output.len());

    step("6. SSH 连接失败路径（端口不可达应给出中文错误）");
    let unreachable = Host {
        id: "probe".into(),
        label: "probe".into(),
        host: "127.0.0.1".into(),
        port: 1,
        username: "root".into(),
        auth: AuthConfig {
            kind: AUTH_PASSWORD.into(),
            password: Some("x".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let rt = tokio::runtime::Runtime::new().expect("创建 tokio 运行时");
    let err = match rt.block_on(async { ssh::connect_and_auth(&unreachable).await }) { Ok(_) => panic!("本应连接失败"), Err(e) => e, };
    println!("预期内的连接失败：{err}");
    assert!(err.to_string().contains("无法连接"), "错误信息应可读");

    step("完成");
    println!("全部冒烟测试通过 ✅");
    let _ = std::fs::remove_dir_all(&dir);
}
