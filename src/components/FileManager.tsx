import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  FolderPlus,
  Laptop,
  Pencil,
  RefreshCw,
  RotateCcw,
  Server,
  Trash2,
  X,
  XCircle,
} from "lucide-react";
import { api, onTransferJob } from "../lib/api";
import { formatSize, formatTime, joinPath, parentPath } from "../lib/paths";
import type { EnqueueJob, FileEntry, HostRecord, TransferJob } from "../lib/types";
import { useVault } from "../store/useVault";

type Side = "left" | "right";

/** 两侧各自独立的 SFTP 会话 id（远程 ⇄ 远程 时同时存在两条连接） */
const SESSION: Record<Side, string> = { left: "fm:left", right: "fm:right" };

interface Pane {
  kind: "local" | "remote";
  hostId: string | null;
  path: string;
  entries: FileEntry[];
  selected: Set<string>;
  loading: boolean;
  error: string | null;
}

const emptyPane = (kind: "local" | "remote"): Pane => ({
  kind,
  hostId: null,
  path: "",
  entries: [],
  selected: new Set(),
  loading: false,
  error: null,
});

const STATUS_TEXT: Record<TransferJob["status"], string> = {
  pending: "排队中",
  running: "传输中",
  done: "完成",
  error: "失败",
  canceled: "已取消",
};

const KIND_ICON: Record<string, string> = {
  upload: "⤴",
  download: "⤵",
  remoteToRemote: "⇄",
  localToLocal: "→",
};

interface PaneViewProps {
  pane: Pane;
  hosts: HostRecord[];
  onSwitch: (kind: "local" | "remote", hostId: string | null) => void;
  onNavigate: (path: string) => void;
  onRefresh: () => void;
  onSelect: (name: string, e: React.MouseEvent) => void;
  onOpen: (entry: FileEntry) => void;
  onMkdir: () => void;
  onRename: (entry: FileEntry) => void;
  onDelete: (entry: FileEntry) => void;
  extra?: ReactNode;
}

function PaneView(p: PaneViewProps) {
  const [draft, setDraft] = useState(p.pane.path);
  useEffect(() => setDraft(p.pane.path), [p.pane.path]);
  const selectedHost = p.hosts.find((h) => h.id === p.pane.hostId);

  return (
    <section className="fm-pane">
      <header className="fm-pane-head">
        <select
          className="fm-source"
          value={p.pane.kind === "local" ? "__local__" : p.pane.hostId ?? ""}
          onChange={(e) => {
            const v = e.target.value;
            if (v === "__local__") p.onSwitch("local", null);
            else p.onSwitch("remote", v);
          }}
        >
          <option value="__local__">💻 本地磁盘</option>
          {p.hosts.map((h) => (
            <option key={h.id} value={h.id}>
              🖥 {h.label || `${h.username}@${h.host}`}
            </option>
          ))}
        </select>
        <div className="fm-head-actions">
          <button title="上一级" onClick={() => p.onNavigate(parentPath(p.pane.path))}>
            <ArrowUp size={14} />
          </button>
          <button title="刷新" onClick={p.onRefresh}>
            <RefreshCw size={13} className={p.pane.loading ? "spin" : undefined} />
          </button>
          <button title="新建文件夹" onClick={p.onMkdir}>
            <FolderPlus size={14} />
          </button>
        </div>
      </header>

      <div className="fm-path">
        <input
          value={draft}
          spellCheck={false}
          placeholder={p.pane.kind === "local" ? "本地路径" : "远程路径"}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") p.onNavigate(draft.trim());
          }}
        />
        {selectedHost && (
          <span className="fm-host-tag">
            <Server size={11} /> {selectedHost.username}@{selectedHost.host}
          </span>
        )}
      </div>

      {p.pane.error && <div className="fm-error">{p.pane.error}</div>}

      <div className="fm-list">
        {p.pane.loading && p.pane.entries.length === 0 && <div className="fm-empty">加载中…</div>}
        {!p.pane.loading && p.pane.entries.length === 0 && (
          <div className="fm-empty">（空目录）</div>
        )}
        {p.pane.entries.map((f) => (
          <div
            key={f.path}
            className={`fm-row${p.pane.selected.has(f.name) ? " selected" : ""}`}
            onClick={(e) => p.onSelect(f.name, e)}
            onDoubleClick={() => p.onOpen(f)}
            title={f.path}
          >
            <span className="fm-icon">{f.isDir ? "📁" : "📄"}</span>
            <span className="fm-name">{f.name}</span>
            <span className="fm-size">{f.isDir ? "" : formatSize(f.size)}</span>
            <span className="fm-time">{formatTime(f.modified)}</span>
            <span className="fm-row-actions">
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

      <footer className="fm-pane-foot">
        <span>
          {p.pane.kind === "local" ? <Laptop size={12} /> : <Server size={12} />}
          {p.pane.entries.length} 项
          {p.pane.selected.size > 0 ? ` · 已选 ${p.pane.selected.size}` : ""}
        </span>
        {p.extra}
      </footer>
    </section>
  );
}

function QueuePanel({
  jobs,
  onCancel,
  onRetry,
  onClear,
}: {
  jobs: TransferJob[];
  onCancel: (id: string) => void;
  onRetry: (id: string) => void;
  onClear: () => void;
}) {
  const active = jobs.filter((j) => j.status === "running" || j.status === "pending").length;
  return (
    <div className="fm-queue">
      <div className="fm-queue-head">
        <span>
          传输队列 · {active} 个进行中 / 共 {jobs.length} 项
          <span className="fm-queue-tip">（后台运行，关闭窗口也会继续传）</span>
        </span>
        <button className="fm-mini" onClick={onClear}>
          清理已完成
        </button>
      </div>
      {jobs.length === 0 && <div className="fm-note">还没有传输任务</div>}
      <div className="fm-transfers">
        {jobs.map((j) => {
          const pct = j.total > 0 ? Math.min(100, Math.round((j.done / j.total) * 100)) : 0;
          return (
            <div className="fm-transfer" key={j.id}>
              <span className="fm-t-name" title={`${j.srcPath} → ${j.dstPath}`}>
                {KIND_ICON[j.kind] ?? "→"} {j.name}
              </span>
              <span className="fm-t-bar">
                <i
                  className={j.status === "error" ? "err" : undefined}
                  style={{ width: `${j.status === "done" ? 100 : pct}%` }}
                />
              </span>
              <span className="fm-t-text">
                {STATUS_TEXT[j.status]}
                {j.status === "running" || j.status === "pending"
                  ? ` ${pct}% ${formatSize(j.done)}${j.total ? ` / ${formatSize(j.total)}` : ""}`
                  : j.status === "done"
                    ? ` ${formatSize(j.done)}`
                    : ""}
                {j.message && j.status !== "running" ? ` · ${j.message}` : ""}
              </span>
              <span className="fm-t-actions">
                {(j.status === "running" || j.status === "pending") && (
                  <button title="取消" onClick={() => onCancel(j.id)}>
                    <XCircle size={12} />
                  </button>
                )}
                {(j.status === "error" || j.status === "canceled") && (
                  <button title="重试（断点续传）" onClick={() => onRetry(j.id)}>
                    <RotateCcw size={12} />
                  </button>
                )}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

export function FileManager() {
  const hosts = useVault((s) => s.hosts);
  const [left, setLeft] = useState<Pane>(emptyPane("local"));
  const [right, setRight] = useState<Pane>(emptyPane("remote"));
  const [toast, setToast] = useState<string | null>(null);
  const [jobs, setJobs] = useState<TransferJob[]>([]);

  const leftRef = useRef(left);
  const rightRef = useRef(right);
  useEffect(() => {
    leftRef.current = left;
  }, [left]);
  useEffect(() => {
    rightRef.current = right;
  }, [right]);

  const getPane = (side: Side) => (side === "left" ? leftRef.current : rightRef.current);
  const setPane = useCallback((side: Side, patch: Partial<Pane>) => {
    const setter = side === "left" ? setLeft : setRight;
    setter((prev) => ({ ...prev, ...patch }));
  }, []);

  /**
   * 队列里的任务按 id 去重写入。
   * 之前 enqueue 的返回值 + 后端推来的 transfer-job 事件会各插一次，
   * 导致同一个任务出现两条（一条跟着事件更新、一条永远停在 0%）。
   */
  const upsertJob = useCallback((job: TransferJob) => {
    setJobs((prev) => {
      const rest = prev.filter((j) => j.id !== job.id);
      return [...rest, job];
    });
  }, []);

  const load = useCallback(
    async (side: Side, override?: Partial<Pane>) => {
      const pane: Pane = { ...getPane(side), ...(override ?? {}) };
      setPane(side, { ...(override ?? {}), loading: true, error: null });
      try {
        if (pane.kind === "local") {
          let path = pane.path;
          if (!path) {
            const home = await api.localHome();
            path = home.home;
          }
          const entries = await api.localList(path);
          setPane(side, { entries, path, loading: false, selected: new Set() });
        } else {
          if (!pane.hostId) throw new Error("请先选择远程连接");
          const sid = SESSION[side];
          const home = await api.sftpOpen(sid, pane.hostId);
          const path = pane.path || home || "/";
          const entries = await api.sftpList(sid, path);
          setPane(side, { entries, path, loading: false, selected: new Set() });
        }
      } catch (e) {
        setPane(side, { loading: false, error: String(e) });
      }
    },
    [setPane],
  );

  const switchTo = useCallback(
    async (side: Side, kind: "local" | "remote", hostId: string | null) => {
      setPane(side, { kind, hostId, path: "", entries: [], selected: new Set(), error: null });
      await api.sftpClose(SESSION[side]).catch(() => undefined);
      await load(side, { kind, hostId, path: "" });
    },
    [load, setPane],
  );

  const started = useRef(false);
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    (async () => {
      await useVault.getState().load();
      const hs = useVault.getState().hosts;
      await load("left", { kind: "local", hostId: null, path: "" });
      const wanted = new URLSearchParams(location.search).get("host");
      const target = wanted && hs.some((h) => h.id === wanted) ? wanted : hs[0]?.id ?? null;
      if (target) await load("right", { kind: "remote", hostId: target, path: "" });
      else await load("right", { kind: "local", hostId: null, path: "" });
      setJobs(await api.listTransfers().catch(() => []));
    })().catch((e) => setToast(String(e)));
  }, [load]);

  // 主窗口再点"文件"时聚焦本窗口并切换左侧主机
  useEffect(() => {
    let un: (() => void) | undefined;
    listen<string | null>("fm-open", (ev) => {
      if (ev.payload) void switchTo("left", "remote", ev.payload);
    })
      .then((u) => {
        un = u;
      })
      .catch(() => undefined);
    return () => un?.();
  }, [switchTo]);

  // 订阅后台传输队列
  useEffect(() => {
    let un: (() => void) | undefined;
    const timers = new Map<string, number>();
    onTransferJob((job) => {
      upsertJob(job);
      // 任务结束/失败时刷新两侧列表（新文件已落地）
      if (job.status === "done" || job.status === "error") {
        void load("left");
        void load("right");
      }
      // 成功的任务在列表里停留 4 秒后自动移除（失败/取消的保留，方便重试）
      if (job.status === "done") {
        const old = timers.get(job.id);
        if (old) window.clearTimeout(old);
        timers.set(
          job.id,
          window.setTimeout(() => {
            setJobs((prev) => prev.filter((j) => j.id !== job.id));
            timers.delete(job.id);
          }, 4000),
        );
      }
    })
      .then((u) => {
        un = u;
      })
      .catch(() => undefined);
    return () => {
      un?.();
      timers.forEach((t) => window.clearTimeout(t));
    };
  }, [load, upsertJob]);

  const select = (side: Side, name: string, e: React.MouseEvent) => {
    const pane = getPane(side);
    const next = new Set(e.ctrlKey || e.metaKey || e.shiftKey ? pane.selected : []);
    if (next.has(name) && (e.ctrlKey || e.metaKey)) next.delete(name);
    else next.add(name);
    setPane(side, { selected: next });
  };

  const doMkdir = async (side: Side) => {
    const pane = getPane(side);
    const name = window.prompt("新建文件夹名称：");
    if (!name) return;
    try {
      const target = joinPath(pane.path, name);
      if (pane.kind === "local") await api.localMkdir(target);
      else await api.sftpMkdir(SESSION[side], target);
      await load(side);
    } catch (e) {
      setPane(side, { error: String(e) });
    }
  };

  const doRename = async (side: Side, entry: FileEntry) => {
    const pane = getPane(side);
    const name = window.prompt("重命名为：", entry.name);
    if (!name || name === entry.name) return;
    try {
      const to = joinPath(parentPath(entry.path), name);
      if (pane.kind === "local") await api.localRename(entry.path, to);
      else await api.sftpRename(SESSION[side], entry.path, to);
      await load(side);
    } catch (e) {
      setPane(side, { error: String(e) });
    }
  };

  const doDelete = async (side: Side, entry: FileEntry) => {
    const pane = getPane(side);
    if (
      !window.confirm(
        `确认删除 ${entry.name}？${entry.isDir ? "（文件夹及其全部内容会被递归删除）" : ""}`,
      )
    )
      return;
    try {
      if (pane.kind === "local") await api.localRemove(entry.path);
      else await api.sftpRemove(SESSION[side], entry.path, entry.isDir);
      await load(side);
    } catch (e) {
      setPane(side, { error: String(e) });
    }
  };

  /** 把 from 侧选中的内容加入后台传输队列 */
  const doTransfer = async (from: Side) => {
    const to: Side = from === "left" ? "right" : "left";
    const src = getPane(from);
    const dst = getPane(to);
    if (src.selected.size === 0) {
      setToast("请先在源面板选中文件或文件夹");
      return;
    }
    if (!src.path || !dst.path) {
      setToast("源或目标目录还没加载完成");
      return;
    }
    if (dst.kind === "remote" && !dst.hostId) {
      setToast("目标面板还没有选择远程连接");
      return;
    }
    if (src.kind === "remote" && !src.hostId) {
      setToast("源面板还没有选择远程连接");
      return;
    }

    const kind: EnqueueJob["kind"] =
      src.kind === "local" && dst.kind === "remote"
        ? "upload"
        : src.kind === "remote" && dst.kind === "local"
          ? "download"
          : src.kind === "remote" && dst.kind === "remote"
            ? "remoteToRemote"
            : "localToLocal";

    const enqueued: EnqueueJob[] = [...src.selected].map((name) => ({
      name,
      kind,
      srcHost: src.kind === "remote" ? src.hostId : null,
      srcPath: joinPath(src.path, name),
      dstHost: dst.kind === "remote" ? dst.hostId : null,
      dstPath: joinPath(dst.path, name),
    }));

    try {
      const created = await api.enqueueTransfers(enqueued);
      created.forEach(upsertJob);
      setPane(from, { selected: new Set() });
      setToast(`已加入队列：${created.length} 项（后台传输，可关闭本窗口）`);
    } catch (e) {
      setToast(String(e));
    }
  };

  const paneProps = (side: Side) => {
    const pane = side === "left" ? left : right;
    return {
      pane,
      hosts,
      onSwitch: (kind: "local" | "remote", hostId: string | null) =>
        void switchTo(side, kind, hostId),
      onNavigate: (path: string) => void load(side, { path }),
      onRefresh: () => void load(side),
      onSelect: (name: string, e: React.MouseEvent) => select(side, name, e),
      onOpen: (entry: FileEntry) => {
        if (entry.isDir) void load(side, { path: entry.path });
      },
      onMkdir: () => void doMkdir(side),
      onRename: (entry: FileEntry) => void doRename(side, entry),
      onDelete: (entry: FileEntry) => void doDelete(side, entry),
    };
  };

  const leftHost = useMemo(() => hosts.find((h) => h.id === left.hostId), [hosts, left.hostId]);
  const rightHost = useMemo(() => hosts.find((h) => h.id === right.hostId), [hosts, right.hostId]);

  return (
    <div className="fm-root">
      <header className="fm-top">
        <span className="fm-brand">🍅 文件管理</span>
        <span className="fm-sub">
          两侧都可以选「本地磁盘」或任意远程主机 —— 两边都是远程时即服务器之间直传（不经过本机磁盘）；
          传输走后台队列，关闭本窗口也会继续
        </span>
        {toast && (
          <button className="fm-toast" onClick={() => setToast(null)}>
            {toast} <X size={12} />
          </button>
        )}
      </header>

      <div className="fm-body">
        <PaneView {...paneProps("left")} />

        <div className="fm-middle">
          <button
            title={`把左侧选中项传到右侧${rightHost ? `（${rightHost.label}）` : ""}`}
            onClick={() => void doTransfer("left")}
          >
            <ArrowRight size={18} />
          </button>
          <span className="fm-arrow-label">加入队列</span>
          <button
            title={`把右侧选中项传到左侧${leftHost ? `（${leftHost.label}）` : ""}`}
            onClick={() => void doTransfer("right")}
          >
            <ArrowLeft size={18} />
          </button>
        </div>

        <PaneView {...paneProps("right")} />
      </div>

      <footer className="fm-bottom">
        <QueuePanel
          jobs={jobs}
          onCancel={(id) => {
            api.cancelTransfer(id).catch((e) => setToast(String(e)));
          }}
          onRetry={(id) => {
            api.retryTransfer(id).catch((e) => setToast(String(e)));
          }}
          onClear={() => {
            // 以 Rust 侧队列为准，避免本地残留"幽灵"记录
            api
              .clearFinishedTransfers()
              .then(() => api.listTransfers())
              .then((list) => setJobs(list))
              .catch((e) => setToast(String(e)));
          }}
        />
      </footer>
    </div>
  );
}
