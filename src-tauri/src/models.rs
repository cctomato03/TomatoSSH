use serde::{Deserialize, Serialize};

pub const AUTH_PASSWORD: &str = "password";
pub const AUTH_KEY: &str = "privateKey";
pub const AUTH_AGENT: &str = "agent";

fn default_keepalive() -> u64 {
    30
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct AuthConfig {
    /// password | privateKey | agent
    #[serde(rename = "type")]
    pub kind: String,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub passphrase: Option<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            kind: AUTH_PASSWORD.into(),
            password: None,
            private_key_path: None,
            passphrase: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Host {
    pub id: String,
    pub group_id: Option<String>,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthConfig,
    pub color: Option<String>,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub keepalive_secs: u64,
    /// 跳板机（先连它，再通过它连目标主机）
    pub jump_host_id: Option<String>,
    /// 端口转发规则（本机 → 目标）
    pub forwards: Vec<PortForwardRule>,
    /// TOFU：首次连接时记录的主机公钥指纹
    pub host_key_fingerprint: Option<String>,
    pub updated_at: i64,
    pub deleted: bool,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            id: String::new(),
            group_id: None,
            label: String::new(),
            host: String::new(),
            port: 22,
            username: String::new(),
            auth: AuthConfig::default(),
            color: None,
            tags: Vec::new(),
            notes: None,
            keepalive_secs: default_keepalive(),
            jump_host_id: None,
            forwards: Vec::new(),
            host_key_fingerprint: None,
            updated_at: 0,
            deleted: false,
        }
    }
}

impl Host {
    pub fn display_name(&self) -> String {
        if !self.label.is_empty() {
            self.label.clone()
        } else {
            format!("{}@{}", self.username, self.host)
        }
    }
}

/// 新建 / 编辑连接时的草稿（id 为空表示新建）
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct HostDraft {
    pub id: Option<String>,
    pub group_id: Option<String>,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthConfig,
    pub color: Option<String>,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub keepalive_secs: u64,
    pub jump_host_id: Option<String>,
    pub forwards: Vec<PortForwardRule>,
}

impl Default for HostDraft {
    fn default() -> Self {
        Self {
            id: None,
            group_id: None,
            label: String::new(),
            host: String::new(),
            port: 22,
            username: String::new(),
            auth: AuthConfig::default(),
            color: None,
            tags: Vec::new(),
            notes: None,
            keepalive_secs: default_keepalive(),
            jump_host_id: None,
            forwards: Vec::new(),
        }
    }
}

/// 端口转发规则（本机监听 → 通过 SSH 连到目标）
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct PortForwardRule {
    pub id: String,
    /// 本地监听地址，一般 127.0.0.1
    pub bind_host: String,
    pub bind_port: u16,
    pub dest_host: String,
    pub dest_port: u16,
    /// 打开该连接的终端时自动启动
    pub auto_start: bool,
}

impl Default for PortForwardRule {
    fn default() -> Self {
        Self {
            id: String::new(),
            bind_host: "127.0.0.1".into(),
            bind_port: 0,
            dest_host: "127.0.0.1".into(),
            dest_port: 0,
            auto_start: false,
        }
    }
}

/// 快捷指令（SSH / 本地终端 / 串口通用）
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Snippet {
    pub id: String,
    pub name: String,
    /// 要写入终端的内容（可以多行）
    pub command: String,
    /// 点击后是否自动回车执行
    pub auto_run: bool,
    pub updated_at: i64,
    pub deleted: bool,
}

impl Default for Snippet {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            command: String::new(),
            auto_run: true,
            updated_at: 0,
            deleted: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Group {    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub color: Option<String>,
    pub sort_order: i32,
    pub updated_at: i64,
    pub deleted: bool,
}

impl Default for Group {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            parent_id: None,
            color: None,
            sort_order: 0,
            updated_at: 0,
            deleted: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct GroupDraft {
    pub id: Option<String>,
    pub name: String,
    pub parent_id: Option<String>,
    pub color: Option<String>,
    pub sort_order: Option<i32>,
}

impl Default for GroupDraft {
    fn default() -> Self {
        Self {
            id: None,
            name: String::new(),
            parent_id: None,
            color: None,
            sort_order: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncSettings {
    pub server_url: String,
    pub email: String,
    pub auto_sync: bool,
    pub last_sync_at: Option<i64>,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            email: String::new(),
            auto_sync: false,
            last_sync_at: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: String,
    pub font_family: String,
    pub font_size: u16,
    pub line_height: f32,
    pub cursor_blink: bool,
    pub scrollback: u32,
    pub copy_on_select: bool,
    pub sync: SyncSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "tomato-dark".into(),
            font_family: "JetBrains Mono, Fira Code, Cascadia Code, Menlo, Consolas, monospace".into(),
            font_size: 14,
            line_height: 1.25,
            cursor_blink: true,
            scrollback: 8000,
            copy_on_select: false,
            sync: SyncSettings::default(),
        }
    }
}

/// 只存在本机、绝不上传的敏感数据
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DeviceSecrets {
    pub sync_token: Option<String>,
    /// 由账号密码派生出的同步加密密钥（base64 编码的 32 字节），本机保存
    pub sync_key: Option<String>,
    pub device_id: String,
    /// 云端当前版本号（用于冲突检测）
    pub sync_revision: Option<i64>,
}

/// 串口连接配置（只做终端交互，不做文件传输）
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct SerialProfile {
    pub id: String,
    pub label: String,
    /// Windows: COM3 / Linux: /dev/ttyUSB0 / macOS: /dev/cu.usbserial-*
    pub port_name: String,
    pub baud_rate: u32,
    /// 5 | 6 | 7 | 8
    pub data_bits: u8,
    /// none | odd | even
    pub parity: String,
    /// 1 | 2
    pub stop_bits: u8,
    /// none | software | hardware
    pub flow_control: String,
    /// 掉线（USB 被拔等）后是否自动重连
    pub auto_reconnect: bool,
    /// 上次成功打开的端口名（USB 重新枚举成 COMx 变了也能找回）
    pub last_port_name: Option<String>,
    /// 选端口时记录的 USB VID/PID，用于断线后重新定位设备
    pub usb_vid: Option<u16>,
    pub usb_pid: Option<u16>,
    pub updated_at: i64,
    pub deleted: bool,
}

impl Default for SerialProfile {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            port_name: String::new(),
            baud_rate: 115_200,
            data_bits: 8,
            parity: "none".into(),
            stop_bits: 1,
            flow_control: "none".into(),
            auto_reconnect: true,
            last_port_name: None,
            usb_vid: None,
            usb_pid: None,
            updated_at: 0,
            deleted: false,
        }
    }
}

impl SerialProfile {
    pub fn display_name(&self) -> String {
        if self.label.is_empty() {
            format!("{} @ {}", self.port_name, self.baud_rate)
        } else {
            self.label.clone()
        }
    }
}

/// 系统里可用的串口
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SerialPortInfo {
    pub name: String,
    /// usb | bluetooth | pci | unknown
    pub kind: String,
    pub detail: Option<String>,
}

/// 参与云同步的数据（不含本机密钥）
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct VaultData {
    pub hosts: Vec<Host>,
    pub groups: Vec<Group>,
    pub serials: Vec<SerialProfile>,
    pub snippets: Vec<Snippet>,
    pub settings: Settings,
    pub secrets: DeviceSecrets,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub hosts: Vec<Host>,
    pub groups: Vec<Group>,
    pub serials: Vec<SerialProfile>,
    pub snippets: Vec<Snippet>,
    pub settings: Settings,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: u64,
    pub mode: u32,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub id: String,
    pub file_name: String,
    pub direction: String,
    pub done: u64,
    pub total: u64,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SyncStateDto {
    pub logged_in: bool,
    pub email: Option<String>,
    pub server_url: Option<String>,
    pub revision: Option<i64>,
    pub last_sync_at: Option<i64>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub revision: i64,
    pub uploaded: usize,
    pub downloaded: usize,
    pub message: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct LocalHome {
    pub home: String,
    pub downloads: String,
    pub desktop: String,
    #[serde(rename = "isWindows")]
    pub is_windows: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SshTarget {
    pub host: String,
    pub username: String,
}

pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
