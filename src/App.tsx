import { useEffect } from "react";
import { Plus, Terminal, Server } from "lucide-react";
import { api, onTransferProgress } from "./lib/api";
import { useTabs } from "./store/useTabs";
import { useTransfers } from "./store/useTransfers";
import { useUi } from "./store/useUi";
import { useVault } from "./store/useVault";
import { Sidebar } from "./components/Sidebar";
import { TabBar } from "./components/TabBar";
import { TerminalPane } from "./components/TerminalPane";
import { TransferBar } from "./components/TransferBar";
import { HostDialog } from "./components/HostDialog";
import { GroupDialog } from "./components/GroupDialog";
import { SettingsDialog } from "./components/SettingsDialog";
import { SyncDialog } from "./components/SyncDialog";
import "./styles/global.css";

function EmptyState() {
  const openHostDialog = useUi((s) => s.openHost);
  const openLocalTab = useTabs((s) => s.openLocalTab);

  return (
    <div className="empty-state">
      <div className="empty-logo">🍅</div>
      <h2>TomatoSSH</h2>
      <div>左侧选择一个连接双击即可打开终端，或先创建一个新连接</div>
      <div style={{ display: "flex", gap: 8, marginTop: 6 }}>
        <button className="btn primary" onClick={() => openHostDialog(null, null)}>
          <Plus size={14} /> 新建 SSH 连接
        </button>
        <button className="btn" onClick={() => openLocalTab()}>
          <Terminal size={14} /> 打开本地终端
        </button>
      </div>
      <div className="hint" style={{ marginTop: 10 }}>
        支持多标签并行会话、SFTP 双向传输与跨主机互传、8 套内置主题
      </div>
    </div>
  );
}

export default function App() {
  const loaded = useVault((s) => s.loaded);
  const error = useVault((s) => s.error);
  const load = useVault((s) => s.load);
  const tabs = useTabs((s) => s.tabs);
  const activeId = useTabs((s) => s.activeId);

  useEffect(() => {
    void load();
  }, [load]);

  // 传输进度
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    onTransferProgress((p) => useTransfers.getState().upsert(p))
      .then((un) => {
        unlisten = un;
      })
      .catch(() => undefined);
    return () => unlisten?.();
  }, []);

  // 启动后自动同步（若开启）
  useEffect(() => {
    if (!loaded) return;
    const { settings } = useVault.getState();
    if (!settings.sync.autoSync || !settings.sync.serverUrl) return;
    api
      .syncNow()
      .then(() => useVault.getState().refreshFromServer())
      .catch((e) => console.warn("自动同步失败：", e));
    // 仅在首次加载完成后执行一次
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loaded]);

  // 快捷键
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (!mod) return;
      if (e.key.toLowerCase() === "t" && !e.shiftKey) {
        e.preventDefault();
        useTabs.getState().openLocalTab();
      } else if (e.key.toLowerCase() === "w" && e.shiftKey) {
        const id = useTabs.getState().activeId;
        if (id) {
          e.preventDefault();
          useTabs.getState().closeTab(id);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="app">
      <Sidebar />
      <main className="workspace">
        <TabBar />
        <div className="pane-area">
          {!loaded ? (
            <div className="empty-state">正在加载保险库…</div>
          ) : tabs.length === 0 ? (
            <EmptyState />
          ) : (
            tabs.map((tab) => <TerminalPane key={tab.id} tab={tab} active={tab.id === activeId} />)
          )}
        </div>
        <TransferBar />
      </main>

      <HostDialog />
      <GroupDialog />
      <SettingsDialog />
      <SyncDialog />

      {error && (
        <div className="term-notice" style={{ left: "50%", bottom: 40, top: "auto" }}>
          <Server size={14} />
          <span style={{ whiteSpace: "pre-wrap" }}>{error}</span>
          <button onClick={() => useVault.setState({ error: null })}>关闭</button>
        </div>
      )}
    </div>
  );
}
