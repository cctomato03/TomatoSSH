import { FolderTree, Plus, TerminalSquare, X, Files } from "lucide-react";
import { useTabs } from "../store/useTabs";
import { useVault } from "../store/useVault";

export function TabBar() {
  const tabs = useTabs((s) => s.tabs);
  const activeId = useTabs((s) => s.activeId);
  const setActive = useTabs((s) => s.setActive);
  const closeTab = useTabs((s) => s.closeTab);
  const openLocalTab = useTabs((s) => s.openLocalTab);
  const toggleFiles = useTabs((s) => s.toggleFiles);
  const hosts = useVault((s) => s.hosts);

  const active = tabs.find((t) => t.id === activeId);

  return (
    <div className="tabbar">
      {tabs.map((tab) => {
        const host = tab.hostId ? hosts.find((h) => h.id === tab.hostId) : null;
        return (
          <div
            key={tab.id}
            className={`tab${tab.id === activeId ? " active" : ""}`}
            onClick={() => setActive(tab.id)}
            onAuxClick={(e) => {
              if (e.button === 1) closeTab(tab.id);
            }}
            title={host ? `${host.username}@${host.host}` : tab.title}
          >
            {tab.kind === "local" ? <TerminalSquare size={13} /> : <FolderTree size={13} />}
            <span className="tab-title">{tab.title}</span>
            {tab.status === "connecting" && <span className="dot idle" />}
            {tab.status === "connected" && <span className="dot online" />}
            {tab.status === "error" && <span className="dot error" />}
            <button
              className="tab-close"
              title="关闭标签页"
              onClick={(e) => {
                e.stopPropagation();
                closeTab(tab.id);
              }}
            >
              <X size={12} />
            </button>
          </div>
        );
      })}

      <button className="tab-add" title="新建本地终端" onClick={() => openLocalTab()}>
        <Plus size={15} />
      </button>

      <div className="tabbar-spacer" />

      {active?.kind === "ssh" && (
        <button
          className="tab-add"
          title="文件传输（SFTP）"
          onClick={() => toggleFiles(active.id)}
          style={active.showFiles ? { color: "var(--accent)" } : undefined}
        >
          <Files size={15} />
        </button>
      )}
    </div>
  );
}
