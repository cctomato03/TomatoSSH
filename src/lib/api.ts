import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  EnqueueJob,
  FileEntry,
  GroupRecord,
  HostDraft,
  HostRecord,
  SerialPortInfo,
  SerialProfile,
  SyncResult,
  SyncStateDto,
  TransferJob,
  TransferProgress,
  VaultSnapshot,
} from "./types";

/** 是否运行在 Tauri 容器内（浏览器里跑 vite dev 时为 false） */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) {
    return Promise.reject(new Error("当前不在 TomatoSSH 桌面容器中运行，请使用 `pnpm tauri:dev` 启动。"));
  }
  return invoke<T>(cmd, args);
}

export const api = {
  // ---------- 保险库 / 连接管理 ----------
  loadVault: () => call<VaultSnapshot>("vault_load"),
  hostSave: (draft: HostDraft) => call<HostRecord>("host_save", { draft }),
  hostDelete: (id: string) => call<void>("host_delete", { id }),
  hostClearFingerprint: (id: string) => call<void>("host_clear_fingerprint", { id }),
  groupSave: (draft: Partial<GroupRecord> & { name: string }) => call<GroupRecord>("group_save", { draft }),
  groupDelete: (id: string) => call<void>("group_delete", { id }),
  settingsSave: (settings: AppSettings) => call<void>("settings_save", { settings }),

  // ---------- SSH / 本地终端 ----------
  sshOpen: (sessionId: string, hostId: string, cols: number, rows: number) =>
    call<string>("ssh_open", { sessionId, hostId, cols, rows }),
  sshUserHostKey: (hostId: string) => call<{ host: string; username: string }>("ssh_target", { hostId }),
  localOpen: (sessionId: string, cols: number, rows: number, cwd?: string | null) =>
    call<string>("local_open", { sessionId, cols, rows, cwd: cwd ?? null }),
  termWrite: (sessionId: string, data: string) => call<void>("term_write", { sessionId, data }),
  termResize: (sessionId: string, cols: number, rows: number) =>
    call<void>("term_resize", { sessionId, cols, rows }),
  termClose: (sessionId: string) => call<void>("term_close", { sessionId }),

  // ---------- 远程文件（SFTP） ----------
  sftpOpen: (sessionId: string, hostId: string) => call<string>("sftp_open", { sessionId, hostId }),
  sftpClose: (sessionId: string) => call<void>("sftp_close", { sessionId }),
  sftpHome: (sessionId: string) => call<string>("sftp_home", { sessionId }),
  sftpList: (sessionId: string, path: string) => call<FileEntry[]>("sftp_list", { sessionId, path }),
  sftpMkdir: (sessionId: string, path: string) => call<void>("sftp_mkdir", { sessionId, path }),
  sftpRemove: (sessionId: string, path: string, isDir: boolean) =>
    call<void>("sftp_remove", { sessionId, path, isDir }),
  sftpRename: (sessionId: string, from: string, to: string) =>
    call<void>("sftp_rename", { sessionId, from, to }),
  sftpDownload: (sessionId: string, remotePath: string, localPath: string) =>
    call<void>("sftp_download", { sessionId, remotePath, localPath }),
  sftpUpload: (sessionId: string, localPath: string, remotePath: string) =>
    call<void>("sftp_upload", { sessionId, localPath, remotePath }),
  sftpCopyRemote: (srcSessionId: string, srcPath: string, dstSessionId: string, dstPath: string) =>
    call<void>("sftp_copy_remote", { srcSessionId, srcPath, dstSessionId, dstPath }),

  // ---------- 本地文件 ----------
  localList: (path: string | null) => call<FileEntry[]>("local_list", { path }),
  localHome: () => call<{ home: string; downloads: string; desktop: string; isWindows: boolean }>("local_home"),
  localMkdir: (path: string) => call<void>("local_mkdir", { path }),
  localRemove: (path: string) => call<void>("local_remove", { path }),
  localRename: (from: string, to: string) => call<void>("local_rename", { from, to }),
  localCopy: (from: string, to: string) => call<void>("local_copy", { from, to }),

  // ---------- 独立文件管理器窗口 ----------
  openFileManager: (hostId?: string | null) =>
    call<void>("open_file_manager", { hostId: hostId ?? null }),

  // ---------- 串口 ----------
  serialListPorts: () => call<SerialPortInfo[]>("serial_list_ports"),
  serialSave: (draft: SerialProfile) => call<SerialProfile>("serial_save", { draft }),
  serialDelete: (id: string) => call<void>("serial_delete", { id }),
  serialOpen: (sessionId: string, profileId: string) =>
    call<string>("serial_open", { sessionId, profileId }),

  // ---------- App 信息 ----------
  appVersion: () => call<string>("app_version"),

  // ---------- 后台传输队列 ----------
  enqueueTransfers: (jobs: EnqueueJob[]) => call<TransferJob[]>("enqueue_transfers", { jobs }),
  listTransfers: () => call<TransferJob[]>("list_transfers"),
  cancelTransfer: (id: string) => call<void>("cancel_transfer", { id }),
  retryTransfer: (id: string) => call<void>("retry_transfer", { id }),
  clearFinishedTransfers: () => call<void>("clear_finished_transfers"),

  // ---------- 云同步 ----------
  syncState: () => call<SyncStateDto>("sync_state"),
  syncLogin: (serverUrl: string, email: string, password: string) =>
    call<SyncStateDto>("sync_login", { serverUrl, email, password }),
  syncRegister: (serverUrl: string, email: string, password: string) =>
    call<SyncStateDto>("sync_register", { serverUrl, email, password }),
  syncLogout: () => call<void>("sync_logout"),
  syncPush: () => call<SyncResult>("sync_push"),
  syncPull: () => call<SyncResult>("sync_pull"),
  syncNow: () => call<SyncResult>("sync_now"),
};

/** 终端 stdout 数据流（base64） */
export function onTermData(sessionId: string, cb: (chunk: Uint8Array) => void): Promise<UnlistenFn> {
  return listen<string>(`term-data:${sessionId}`, (e) => cb(base64ToBytes(e.payload)));
}

/** 终端退出 */
export function onTermExit(sessionId: string, cb: (info: { code: number | null; message?: string }) => void) {
  return listen<{ code: number | null; message?: string }>(`term-exit:${sessionId}`, (e) => cb(e.payload));
}

/** 传输进度 */
export function onTransferProgress(cb: (p: TransferProgress) => void): Promise<UnlistenFn> {
  return listen<TransferProgress>("transfer-progress", (e) => cb(e.payload));
}

/** 后台传输队列的任务变化 */
export function onTransferJob(cb: (j: TransferJob) => void): Promise<UnlistenFn> {
  return listen<TransferJob>("transfer-job", (e) => cb(e.payload));
}

export function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export function bytesToBase64(bytes: Uint8Array): string {
  let bin = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    bin += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(bin);
}
