import { useMemo, useState } from "react";
import {
  Cable,
  ChevronDown,
  Waypoints,
  ChevronRight,
  FolderClosed,
  FolderOpen,
  FolderPlus,
  Laptop,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  Search,
  Settings as SettingsIcon,
  Trash2,
  Cloud,
  Copy,
  Server,
} from "lucide-react";
import type { GroupRecord, HostRecord, SerialProfile } from "../lib/types";
import { useTabs } from "../store/useTabs";
import { useVault } from "../store/useVault";
import { api } from "../lib/api";
import { useUi } from "../store/useUi";
import { ContextMenu, type CtxItem } from "./ContextMenu";
import { ForwardsDialog } from "./ForwardsDialog";
import { SerialDialog } from "./SerialDialog";

interface MenuState {
  x: number;
  y: number;
  items: CtxItem[];
}

export function Sidebar() {
  const hosts = useVault((s) => s.hosts);
  const serials = useVault((s) => s.serials);
  const deleteSerial = useVault((s) => s.deleteSerial);
  const groups = useVault((s) => s.groups);
  const saveHost = useVault((s) => s.saveHost);
  const deleteHost = useVault((s) => s.deleteHost);
  const deleteGroup = useVault((s) => s.deleteGroup);

  const tabs = useTabs((s) => s.tabs);
  const activeId = useTabs((s) => s.activeId);
  const activeHostId = useMemo(() => tabs.find((t) => t.id === activeId)?.hostId ?? null, [tabs, activeId]);
  const openSshTab = useTabs((s) => s.openSshTab);
  const openSerialTab = useTabs((s) => s.openSerialTab);
  const openLocalTab = useTabs((s) => s.openLocalTab);

  const openHostDialog = useUi((s) => s.openHost);
  const openGroupDialog = useUi((s) => s.openGroup);
  const setSettingsOpen = useUi((s) => s.setSettingsOpen);
  const setSyncOpen = useUi((s) => s.setSyncOpen);

  const [query, setQuery] = useState("");
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [serialDialog, setSerialDialog] = useState<{ open: boolean; profile: SerialProfile | null }>(
    { open: false, profile: null },
  );
  const [forwardHostId, setForwardHostId] = useState<string | null>(null);

  const activeHostIds = useMemo(
    () => new Set(tabs.filter((t) => t.kind === "ssh" && t.hostId).map((t) => t.hostId as string)),
    [tabs],
  );

  const toggle = (id: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const connect = (host: HostRecord, withFiles = false) => {
    if (withFiles) {
      // 文件传输走独立窗口，不挤在终端右侧
      api.openFileManager(host.id).catch((e) => console.error(e));
      return;
    }
    const existing = tabs.find((t) => t.kind === "ssh" && t.hostId === host.id && t.status !== "error");
    if (existing) {
      useTabs.getState().setActive(existing.id);
      return;
    }
    openSshTab(host);
  };

  const childrenOf = (parentId: string | null) =>
    groups.filter((g) => (g.parentId ?? null) === parentId).sort((a, b) => a.name.localeCompare(b.name));

  const hostsOf = (groupId: string | null) =>
    hosts
      .filter((h) => (h.groupId ?? null) === groupId)
      .sort((a, b) => a.label.localeCompare(b.label));

  const matchedHosts = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    return hosts.filter(
      (h) =>
        h.label.toLowerCase().includes(q) ||
        h.host.toLowerCase().includes(q) ||
        h.username.toLowerCase().includes(q) ||
        h.tags.some((t) => t.toLowerCase().includes(q)),
    );
  }, [hosts, query]);

  const hostMenu = (host: HostRecord, x: number, y: number) => {
    setMenu({
      x,
      y,
      items: [
        { label: "连接", icon: <Play size={13} />, onClick: () => connect(host) },
        { label: "在文件管理器中打开", icon: <FolderOpen size={13} />, onClick: () => connect(host, true) },
        {
          label: "端口转发…",
          icon: <Waypoints size={13} />,
          onClick: () => setForwardHostId(host.id),
        },
        { kind: "sep" },
        {
          label: "编辑",
          icon: <Pencil size={13} />,
          onClick: () => openHostDialog(host),
        },
        {
          label: "复制为新连接",
          icon: <Copy size={13} />,
          onClick: async () => {
            await saveHost({
              label: `${host.label} 副本`,
              host: host.host,
              port: host.port,
              username: host.username,
              auth: host.auth,
              groupId: host.groupId,
              color: host.color,
              tags: host.tags,
              notes: host.notes,
              keepaliveSecs: host.keepaliveSecs,
            });
          },
        },
        { kind: "sep" },
        {
          label: "删除",
          danger: true,
          icon: <Trash2 size={13} />,
          onClick: async () => {
            if (window.confirm(`确认删除连接「${host.label || host.host}」？`)) {
              await deleteHost(host.id);
            }
          },
        },
      ],
    });
  };

  const connectSerial = (profile: SerialProfile) => {
    openSerialTab(profile);
  };

  const serialMenu = (profile: SerialProfile, x: number, y: number) => {
    setMenu({
      x,
      y,
      items: [
        { label: "打开串口终端", icon: <Play size={13} />, onClick: () => connectSerial(profile) },
        { kind: "sep" },
        {
          label: "编辑",
          icon: <Pencil size={13} />,
          onClick: () => setSerialDialog({ open: true, profile }),
        },
        {
          label: "删除",
          danger: true,
          icon: <Trash2 size={13} />,
          onClick: async () => {
            if (window.confirm(`确认删除串口「${profile.label || profile.portName}」？`)) {
              await deleteSerial(profile.id);
            }
          },
        },
      ],
    });
  };

  const groupMenu = (group: GroupRecord | null, x: number, y: number) => {
    const groupId = group?.id ?? null;
    setMenu({
      x,
      y,
      items: [
        { label: "新建连接", icon: <Plus size={13} />, onClick: () => openHostDialog(null, groupId) },
        {
          label: "新建子分组",
          icon: <FolderPlus size={13} />,
          onClick: () => openGroupDialog(null, groupId),
        },
        ...(group
          ? ([
              { kind: "sep" },
              {
                label: "重命名 / 移动",
                icon: <Pencil size={13} />,
                onClick: () => openGroupDialog(group),
              },
              {
                label: "删除分组",
                danger: true,
                icon: <Trash2 size={13} />,
                onClick: async () => {
                  if (window.confirm(`确认删除分组「${group.name}」？组内连接会移动到根目录。`)) {
                    await deleteGroup(group.id);
                  }
                },
              },
            ] as CtxItem[])
          : []),
      ],
    });
  };

  const renderHost = (host: HostRecord, depth: number) => {
    const online = activeHostIds.has(host.id);
    return (
      <div
        key={host.id}
        className={`tree-row${activeHostId === host.id ? " active" : ""}`}
        style={{ paddingLeft: 8 + depth * 14 }}
        onDoubleClick={() => connect(host)}
        onContextMenu={(e) => {
          e.preventDefault();
          hostMenu(host, e.clientX, e.clientY);
        }}
        title={`${host.username}@${host.host}:${host.port}`}
      >
        <span className={`dot ${online ? "online" : "idle"}`} />
        <div className="tree-host">
          <span className="tree-label">{host.label || `${host.username}@${host.host}`}</span>
          <span className="tree-sub">
            {host.username}@{host.host}
            {host.port !== 22 ? `:${host.port}` : ""}
          </span>
        </div>
        <button
          className="tree-play"
          title="连接"
          onClick={(e) => {
            e.stopPropagation();
            connect(host);
          }}
        >
          <Play size={13} />
        </button>
      </div>
    );
  };

  const renderGroup = (group: GroupRecord, depth: number) => {
    const isCollapsed = collapsed.has(group.id);
    const subGroups = childrenOf(group.id);
    const subHosts = hostsOf(group.id);
    return (
      <div key={group.id}>
        <div
          className="tree-row"
          style={{ paddingLeft: 4 + depth * 14 }}
          onClick={() => toggle(group.id)}
          onContextMenu={(e) => {
            e.preventDefault();
            groupMenu(group, e.clientX, e.clientY);
          }}
        >
          <span className="tree-caret">
            {isCollapsed ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
          </span>
          <FolderClosed size={14} style={{ color: group.color ?? "var(--accent)" }} />
          <span className="tree-label" style={{ fontWeight: 500 }}>
            {group.name}
          </span>
          <span className="tree-sub">{subHosts.length || ""}</span>
        </div>
        {!isCollapsed && (
          <>
            {subGroups.map((g) => renderGroup(g, depth + 1))}
            {subHosts.map((h) => renderHost(h, depth + 1))}
          </>
        )}
      </div>
    );
  };

  return (
    <aside className="sidebar" onContextMenu={(e) => e.preventDefault()}>
      <div className="brand">
        <span className="brand-logo">T</span>
        <span>TomatoSSH</span>
        <div className="brand-actions">
          <button className="icon-btn" title="云同步" onClick={() => setSyncOpen(true)}>
            <Cloud size={15} />
          </button>
          <button className="icon-btn" title="设置" onClick={() => setSettingsOpen(true)}>
            <SettingsIcon size={15} />
          </button>
        </div>
      </div>

      <div className="sidebar-search">
        <div style={{ position: "relative" }}>
          <Search
            size={13}
            style={{
              position: "absolute",
              left: 8,
              top: "50%",
              transform: "translateY(-50%)",
              color: "var(--text-muted)",
            }}
          />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="搜索主机 / 用户 / 标签"
            style={{ width: "100%", paddingLeft: 26 }}
          />
        </div>
      </div>

      <div
        className="sidebar-scroll"
        onContextMenu={(e) => {
          if (e.target === e.currentTarget) {
            e.preventDefault();
            groupMenu(null, e.clientX, e.clientY);
          }
        }}
      >
        {query.trim() ? (
          <>
            <div className="ctx-label">搜索结果（{matchedHosts.length}）</div>
            {matchedHosts.map((h) => renderHost(h, 0))}
            {matchedHosts.length === 0 && (
              <div className="hint" style={{ padding: "8px 10px" }}>
                没有匹配的连接
              </div>
            )}
          </>
        ) : (
          <>
            <div
              className="tree-row"
              onClick={() => openLocalTab()}
              title="打开一个本机 Shell"
            >
              <Laptop size={14} />
              <span className="tree-label">本地终端</span>
              <button
                className="tree-play"
                style={{ opacity: 1 }}
                onClick={(e) => {
                  e.stopPropagation();
                  openLocalTab();
                }}
              >
                <Play size={12} />
              </button>
            </div>
            <div style={{ height: 6 }} />
            {childrenOf(null).map((g) => renderGroup(g, 0))}
            {hostsOf(null).map((h) => renderHost(h, 0))}

            {serials.length > 0 && <div className="ctx-label" style={{ marginTop: 10 }}>串口</div>}
            {serials.map((s) => (
              <div
                key={s.id}
                className="tree-row"
                style={{ paddingLeft: 8 }}
                onDoubleClick={() => connectSerial(s)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  serialMenu(s, e.clientX, e.clientY);
                }}
                title={`${s.portName} · ${s.baudRate} ${s.dataBits}${s.parity[0].toUpperCase()}${s.stopBits}`}
              >
                <Cable size={13} style={{ color: "var(--text-muted)" }} />
                <div className="tree-host">
                  <span className="tree-label">{s.label || s.portName}</span>
                  <span className="tree-sub">
                    {s.portName} · {s.baudRate} {s.dataBits}
                    {s.parity[0].toUpperCase()}
                    {s.stopBits}
                  </span>
                </div>
                <button
                  className="tree-play"
                  title="打开串口终端"
                  onClick={(e) => {
                    e.stopPropagation();
                    connectSerial(s);
                  }}
                >
                  <Play size={13} />
                </button>
              </div>
            ))}

            {hosts.length === 0 && groups.length === 0 && serials.length === 0 && (
              <div className="empty-state" style={{ padding: "28px 12px" }}>
                <Server size={22} />
                <div>还没有连接</div>
                <button className="btn primary" onClick={() => openHostDialog(null, null)}>
                  <Plus size={13} /> 新建连接
                </button>
              </div>
            )}
          </>
        )}
      </div>

      <div className="sidebar-footer">
        <button className="btn" style={{ flex: 1 }} onClick={() => openHostDialog(null, null)}>
          <Plus size={13} /> 连接
        </button>
        <button className="btn" style={{ flex: 1 }} onClick={() => openGroupDialog(null, null)}>
          <FolderPlus size={13} /> 分组
        </button>
        <button
          className="icon-btn"
          title="新建串口连接"
          onClick={() => setSerialDialog({ open: true, profile: null })}
        >
          <Cable size={14} />
        </button>
        <button className="icon-btn" title="打开文件管理器" onClick={() => api.openFileManager().catch((e) => console.error(e))}>
          <FolderOpen size={14} />
        </button>
        <button className="icon-btn" title="刷新" onClick={() => useVault.getState().load()}>
          <RefreshCw size={14} />
        </button>
      </div>

      {menu && <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />}

      {forwardHostId && (
        <ForwardsDialog hostId={forwardHostId} onClose={() => setForwardHostId(null)} />
      )}

      {serialDialog.open && (
        <SerialDialog
          profile={serialDialog.profile}
          onClose={() => setSerialDialog({ open: false, profile: null })}
        />
      )}
    </aside>
  );
}
