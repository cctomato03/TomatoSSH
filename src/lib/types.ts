// 与 Rust 端 serde 模型一一对应的类型定义（Rust 侧统一 camelCase 序列化）
export type AuthType = "password" | "privateKey" | "agent";

export interface AuthConfig {
  type: AuthType;
  password?: string | null;
  privateKeyPath?: string | null;
  passphrase?: string | null;
}

/** 端口转发规则（本机监听 → 经 SSH 到目标） */
export interface PortForwardRule {
  id: string;
  bindHost: string;
  bindPort: number;
  destHost: string;
  destPort: number;
  autoStart: boolean;
}

/** 正在运行的转发 */
export interface ActiveForward {
  id: string;
  hostId: string;
  bindHost: string;
  bindPort: number;
  destHost: string;
  destPort: number;
  startedAt: number;
  connections: number;
}

/** 快捷指令 */
export interface Snippet {
  id: string;
  name: string;
  /** 分组名，空字符串表示未分组 */
  group: string;
  command: string;
  autoRun: boolean;
  updatedAt: number;
  deleted: boolean;
}

export interface HostRecord {
  id: string;
  groupId: string | null;
  label: string;
  host: string;
  port: number;
  username: string;
  auth: AuthConfig;
  color: string | null;
  tags: string[];
  notes: string | null;
  keepaliveSecs: number;
  jumpHostId?: string | null;
  forwards?: PortForwardRule[];
  hostKeyFingerprint?: string | null;
  updatedAt: number;
  deleted: boolean;
}

export interface GroupRecord {
  id: string;
  name: string;
  parentId: string | null;
  color: string | null;
  sortOrder: number;
  updatedAt: number;
  deleted: boolean;
}

export interface SyncConfig {
  serverUrl: string;
  email: string;
  autoSync: boolean;
  lastSyncAt: number | null;
}

export interface AppSettings {
  theme: string;
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  cursorBlink: boolean;
  scrollback: number;
  copyOnSelect: boolean;
  /** 串口日志目录（空 = 首次录制时选择） */
  serialLogDir: string;
  /** 串口日志格式：raw | timestamped */
  serialLogMode: string;
  /** 界面缩放（1 = 100%），只影响界面文字 */
  uiScale: number;
  sync: SyncConfig;
}

export interface VaultSnapshot {
  hosts: HostRecord[];
  groups: GroupRecord[];
  serials: SerialProfile[];
  snippets: Snippet[];
  settings: AppSettings;
}

export interface FileEntry {
  name: string;
  path: string;
  isDir: boolean;
  isSymlink: boolean;
  size: number;
  modified: number;
  mode: number;
}

export interface TransferProgress {
  id: string;
  fileName: string;
  direction: "upload" | "download" | "remoteToRemote" | "local";
  done: number;
  total: number;
  status: "running" | "done" | "error";
  message?: string | null;
}

export interface SyncStateDto {
  loggedIn: boolean;
  email: string | null;
  serverUrl: string | null;
  revision: number | null;
  lastSyncAt: number | null;
}

export interface SyncResult {
  revision: number;
  uploaded: number;
  downloaded: number;
  message: string;
}

export interface HostDraft {
  id?: string | null;
  groupId?: string | null;
  label: string;
  host: string;
  port: number;
  username: string;
  auth: AuthConfig;
  color?: string | null;
  tags?: string[];
  notes?: string | null;
  keepaliveSecs?: number;
  jumpHostId?: string | null;
  forwards?: PortForwardRule[];
}

/** 串口连接配置 */
export interface SerialProfile {
  id: string;
  label: string;
  portName: string;
  baudRate: number;
  dataBits: number;
  parity: string;
  stopBits: number;
  flowControl: string;
  /** 掉线后是否自动重连 */
  autoReconnect: boolean;
  lastPortName?: string | null;
  usbVid?: number | null;
  usbPid?: number | null;
  updatedAt: number;
  deleted: boolean;
}

/** 系统里可用的串口 */
/** 串口控制线状态 */
export interface SerialLines {
  dtr: boolean;
  rts: boolean;
  cts: boolean;
  cd: boolean;
}

export interface SerialPortInfo {
  name: string;
  kind: string;
  detail: string | null;
}

/** 后台传输队列里的一个任务 */
export interface TransferJob {
  id: string;
  name: string;
  kind: "upload" | "download" | "remoteToRemote" | "localToLocal";
  srcHost: string | null;
  srcPath: string;
  dstHost: string | null;
  dstPath: string;
  total: number;
  done: number;
  status: "pending" | "running" | "done" | "error" | "canceled";
  message: string | null;
  attempts: number;
  createdAt: number;
}

/** 入队请求 */
export interface EnqueueJob {
  name: string;
  kind: "upload" | "download" | "remoteToRemote" | "localToLocal";
  srcHost: string | null;
  srcPath: string;
  dstHost: string | null;
  dstPath: string;
}

/** 一个标签页：SSH 会话或本地终端 */
export interface TabRecord {
  id: string;
  kind: "ssh" | "local" | "serial";
  title: string;
  hostId: string | null;
  /** 串口连接配置 id（kind === "serial" 时使用） */
  serialId?: string | null;
  status: "connecting" | "connected" | "closed" | "error";
  error?: string | null;
  cwd?: string | null;
}
