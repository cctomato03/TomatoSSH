import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent, type ReactNode } from "react";
import {
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  FolderPlus,
  Pencil,
  RefreshCw,
  Server,
  Trash2,
  Laptop,
  X,
  Send,
} from "lucide-react";
import { api } from "../lib/api";
import { formatSize, formatTime, joinPath, parentPath } from "../lib/paths";
import type { FileEntry } from "../lib/types";
import { useTabs } from "../store/useTabs";
import { useVault } from "../store/useVault";

interface Props {
  tabId: string;
  hostId: string;
}

interface PaneProps {
  title: string;
  icon: ReactNode;
  path: string;
  entries: FileEntry[];
  selected: Set<string>;
  loading: boolean;
  onSelect: (name: string, e: MouseEvent) => void;
  onOpen: (entry: FileEntry) => void;
  onNavigate: (path: string) => void;
  onRefresh: () => void;
  onMkdir: () => void;
  onRename: (entry: FileEntry) => void;
  onDelete: (entry: FileEntry) => void;
  isWindows?: boolean;
}

function Panes(p: PaneProps) {
  const [draft, setDraft] = useState(p.path);
  useEffect(() => setDraft(p.path), [p.path]);

  return (
    <section className="fp-pane">
      <header className="fp-pane-head">
        <span className="fp-title">
          {p.icon}
          {p.title}
        </span>
        <div className="fp-head-actions">
          <button title="上一级" onClick={() => p.onNavigate(parentPath(p.path))}>
            <ArrowUp size={14} />
          </button>
          <button title="刷新" onClick={p.onRefresh}>
            <RefreshCw size={13} className={p.loading ? "spin" : undefined} />
          </button>
          <button title="新建文件夹" onClick={p.onMkdir}>
            <FolderPlus size={14} />
          </button>
        </div>
      </header>
      <div className="fp-path">
        <input
          value={draft}
          spellCheck={false}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") p.onNavigate(draft.trim());
          }}
        />
      </div>
      <div className="fp-list">
        {p.entries.length === 0 && !p.loading && <div className="fp-empty">（空目录）</div>}
        {p.entries.map((f) => (
          <div
            key={f.path}
            className={`fp-row${p.selected.has(f.name) ? " selected" : ""}`}
            onClick={(e) => p.onSelect(f.name, e)}
            onDoubleClick={() => p.onOpen(f)}
            title={f.path}
          >
            <span className={`fp-icon${f.isDir ? " dir" : ""}`}>{f.isDir ? "📁" : "📄"}</span>
            <span className="fp-name">{f.name}</span>
            <span className="fp-size">{f.isDir ? "" : formatSize(f.size)}</span>
            <span className="fp-time">{formatTime(f.modified)}</span>
            <span className="fp-row-actions">
              <button
                title="重命名"
                onClick={(e) => {
                  e.stopPropagation();
                  p.onRename(f);
                }}
              >
                <Pencil size={12} />
              </button>
              <button
                title="删除"
                onClick={(e) => {
                  e.stopPropagation();
                  p.onDelete(f);
                }}
              >
                <Trash2 size={12} />
              </button>
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}

export function FilePanel({ tabId, hostId }: Props) {
  const sftpId = useMemo(() => `sftp-${tabId}`, [tabId]);
  const toggleFiles = useTabs((s) => s.toggleFiles);
  const tabs = useTabs((s) => s.tabs);
  const hosts = useVault((s) => s.hosts);

  const [localPath, setLocalPath] = useState("");
  const [localEntries, setLocalEntries] = useState<FileEntry[]>([]);
  const [localSel, setLocalSel] = useState<Set<string>>(new Set());
  const [localLoading, setLocalLoading] = useState(false);

  const [remotePath, setRemotePath] = useState("");
  const [remoteEntries, setRemoteEntries] = useState<FileEntry[]>([]);
  const [remoteSel, setRemoteSel] = useState<Set<string>>(new Set());
  const [remoteLoading, setRemoteLoading] = useState(false);

  const [peerTabId, setPeerTabId] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const startedRef = useRef(false);

  const host = hosts.find((h) => h.id === hostId);
  const peerTabs = tabs.filter((t) => t.kind === "ssh" && t.id !== tabId && t.status !== "error");

  const loadLocal = useCallback(async (path: string) => {
    setLocalLoading(true);
    try {
      const entries = await api.localList(path || null);
      setLocalEntries(entries);
      setLocalPath(path);
      setLocalSel(new Set());
    } catch (e) {
      setErr(String(e));
    } finally {
      setLocalLoading(false);
    }
  }, []);

  const loadRemote = useCallback(
    async (path: string) => {
      setRemoteLoading(true);
      try {
        const entries = await api.sftpList(sftpId, path);
        setRemoteEntries(entries);
        setRemotePath(path);
        setRemoteSel(new Set());
      } catch (e) {
        setErr(String(e));
      } finally {
        setRemoteLoading(false);
      }
    },
    [sftpId],
  );

  // 首次挂载：建立 SFTP 通道，载入本地/远程目录
  useEffect(() => {
    if (startedRef.current) return;
    startedRef.current = true;
    (async () => {
      try {
        const [home, info] = await Promise.all([api.localHome(), api.sftpOpen(sftpId, hostId)]);
        await loadLocal(home.home);
        const remoteHome = await api.sftpHome(sftpId).catch(() => info || "/");
        await loadRemote(remoteHome || "/");
      } catch (e) {
        setErr(String(e));
      }
    })();
  }, [hostId, sftpId, loadLocal, loadRemote]);

  const select = (name: string, e: MouseEvent, set: Set<string>, apply: (s: Set<string>) => void) => {    const next = new Set(e.ctrlKey || e.metaKey || e.shiftKey ? set : []);
    if (next.has(name) && (e.ctrlKey || e.metaKey)) next.delete(name);
    else next.add(name);
    apply(next);
  };

  const openEntry = (entry: FileEntry, nav: (p: string) => void) => {
    if (entry.isDir) nav(entry.path);
  };

  const join = (dir: string, name: string) => joinPath(dir, name);

  const doUpload = async () => {
    const names = [...localSel];
    if (!names.length) return setErr("请先在左侧选中要上传的文件或文件夹");
    if (!remotePath) return;
    setErr(null);
    for (const name of names) {
      const src = join(localPath, name);
      const dst = join(remotePath, name);
      setBusy(`上传 ${name}`);
      try {
        await api.sftpUpload(sftpId, src, dst);
      } catch (e) {
        setErr(`上传 ${name} 失败：${e}`);
      }
    }
    setBusy(null);
    await loadRemote(remotePath);
  };

  const doDownload = async () => {
    const names = [...remoteSel];
    if (!names.length) return setErr("请先在右侧选中要下载的文件或文件夹");
    if (!localPath) return;
    setErr(null);
    for (const name of names) {
      const src = join(remotePath, name);
      const dst = join(localPath, name);
      setBusy(`下载 ${name}`);
      try {
        await api.sftpDownload(sftpId, src, dst);
      } catch (e) {
        setErr(`下载 ${name} 失败：${e}`);
      }
    }
    setBusy(null);
    await loadLocal(localPath);
  };

  /** 远程 → 另一台远程主机 */
  const doRemoteToRemote = async () => {
    const names = [...remoteSel];
    if (!names.length) return setErr("请先在右侧选中要发送的文件或文件夹");
    if (!peerTabId) return setErr("请选择目标连接");
    const peer = tabs.find((t) => t.id === peerTabId);
    if (!peer?.hostId) return;
    setErr(null);
    const peerSftp = `sftp-${peerTabId}`;
    try {
      await api.sftpOpen(peerSftp, peer.hostId);
      const peerHome = await api.sftpHome(peerSftp).catch(() => "/tmp");
      const target = window.prompt(`发送到「${peer.title}」的目标目录：`, peerHome);
      if (!target) return;
      for (const name of names) {
        setBusy(`发送 ${name} → ${peer.title}`);
        try {
          await api.sftpCopyRemote(sftpId, join(remotePath, name), peerSftp, join(target, name));
        } catch (e) {
          setErr(`发送 ${name} 失败：${e}`);
        }
      }
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  };

  const doRemoteMkdir = async () => {
    const name = window.prompt("新建远程文件夹名称：");
    if (!name) return;
    try {
      await api.sftpMkdir(sftpId, join(remotePath, name));
      await loadRemote(remotePath);
    } catch (e) {
      setErr(String(e));
    }
  };

  const doLocalMkdir = async () => {
    const name = window.prompt("新建本地文件夹名称：");
    if (!name) return;
    try {
      await api.localMkdir(join(localPath, name));
      await loadLocal(localPath);
    } catch (e) {
      setErr(String(e));
    }
  };

  const doRename = async (entry: FileEntry, remote: boolean) => {
    const name = window.prompt("重命名为：", entry.name);
    if (!name || name === entry.name) return;
    try {
      if (remote) {
        await api.sftpRename(sftpId, entry.path, join(parentPath(entry.path), name));
        await loadRemote(remotePath);
      } else {
        await api.localRename(entry.path, join(parentPath(entry.path), name));
        await loadLocal(localPath);
      }
    } catch (e) {
      setErr(String(e));
    }
  };

  const doDelete = async (entry: FileEntry, remote: boolean) => {
    if (
      !window.confirm(
        `确认删除 ${entry.name}？${entry.isDir ? "（文件夹及其全部内容会被递归删除）" : ""}`,
      )
    )
      return;
    try {
      if (remote) {
        await api.sftpRemove(sftpId, entry.path, entry.isDir);
        await loadRemote(remotePath);
      } else {
        await api.localRemove(entry.path);
        await loadLocal(localPath);
      }
    } catch (e) {
      setErr(String(e));
    }
  };

  return (
    <div className="filepanel">
      <div className="fp-top">
        <span className="fp-host">
          <Server size={13} /> {host ? `${host.username}@${host.host}` : "远程文件"}
        </span>
        {busy && <span className="fp-busy">{busy}…</span>}
        <button className="fp-close" title="关闭文件面板" onClick={() => toggleFiles(tabId)}>
          <X size={14} />
        </button>
      </div>

      {err && (
        <div className="fp-error" onClick={() => setErr(null)}>
          {err}
          <X size={12} />
        </div>
      )}

      <div className="fp-cols">
        <Panes
          title="本地"
          icon={<Laptop size={13} />}
          path={localPath}
          entries={localEntries}
          selected={localSel}
          loading={localLoading}
          onSelect={(n, e) => select(n, e, localSel, setLocalSel)}
          onOpen={(entry) => openEntry(entry, loadLocal)}
          onNavigate={loadLocal}
          onRefresh={() => loadLocal(localPath)}
          onMkdir={doLocalMkdir}
          onRename={(entry) => doRename(entry, false)}
          onDelete={(entry) => doDelete(entry, false)}
        />

        <div className="fp-arrows">
          <button title="上传选中的文件/文件夹到远程当前目录（文件夹会递归传输）" onClick={doUpload}>
            <ArrowRight size={16} />
          </button>
          <button title="下载选中的文件/文件夹到本地当前目录（文件夹会递归传输）" onClick={doDownload}>
            <ArrowLeft size={16} />
          </button>
        </div>

        <Panes
          title="远程"
          icon={<Server size={13} />}
          path={remotePath}
          entries={remoteEntries}
          selected={remoteSel}
          loading={remoteLoading}
          onSelect={(n, e) => select(n, e, remoteSel, setRemoteSel)}
          onOpen={(entry) => openEntry(entry, loadRemote)}
          onNavigate={loadRemote}
          onRefresh={() => loadRemote(remotePath)}
          onMkdir={doRemoteMkdir}
          onRename={(entry) => doRename(entry, true)}
          onDelete={(entry) => doDelete(entry, true)}
        />
      </div>

      {peerTabs.length > 0 && (
        <div className="fp-peer">
          <Send size={13} />
          <span>远程互传：把右侧选中项发送到</span>
          <select value={peerTabId} onChange={(e) => setPeerTabId(e.target.value)}>
            <option value="">选择目标连接…</option>
            {peerTabs.map((t) => (
              <option key={t.id} value={t.id}>
                {t.title}
              </option>
            ))}
          </select>
          <button onClick={doRemoteToRemote} disabled={!peerTabId || remoteSel.size === 0}>
            发送
          </button>
        </div>
      )}
    </div>
  );
}

