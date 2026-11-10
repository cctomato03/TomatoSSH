import { create } from "zustand";
import { api } from "../lib/api";
import type { HostRecord, TabRecord } from "../lib/types";

interface TabsState {
  tabs: TabRecord[];
  activeId: string | null;
  openSshTab: (host: HostRecord) => string;
  openLocalTab: (title?: string) => string;
  closeTab: (id: string) => void;
  setActive: (id: string) => void;
  patchTab: (id: string, patch: Partial<TabRecord>) => void;
  toggleFiles: (id: string) => void;
}

const uid = () => crypto.randomUUID();

export const useTabs = create<TabsState>((set, get) => ({
  tabs: [],
  activeId: null,

  openSshTab(host) {
    const id = uid();
    const tab: TabRecord = {
      id,
      kind: "ssh",
      title: host.label || `${host.username}@${host.host}`,
      hostId: host.id,
      status: "connecting",
      showFiles: false,
    };
    set((s) => ({ tabs: [...s.tabs, tab], activeId: id }));
    return id;
  },

  openLocalTab(title) {
    const id = uid();
    const tab: TabRecord = {
      id,
      kind: "local",
      title: title ?? "本地终端",
      hostId: null,
      status: "connecting",
      showFiles: false,
    };
    set((s) => ({ tabs: [...s.tabs, tab], activeId: id }));
    return id;
  },

  closeTab(id) {
    api.termClose(id).catch(() => undefined);
    api.sftpClose(`sftp-${id}`).catch(() => undefined);
    set((s) => {
      const tabs = s.tabs.filter((t) => t.id !== id);
      const activeId = s.activeId === id ? (tabs.length ? tabs[tabs.length - 1].id : null) : s.activeId;
      return { tabs, activeId };
    });
  },

  setActive(id) {
    set({ activeId: id });
  },

  patchTab(id, patch) {
    set((s) => ({ tabs: s.tabs.map((t) => (t.id === id ? { ...t, ...patch } : t)) }));
  },

  toggleFiles(id) {
    const tab = get().tabs.find((t) => t.id === id);
    if (!tab) return;
    set((s) => ({ tabs: s.tabs.map((t) => (t.id === id ? { ...t, showFiles: !t.showFiles } : t)) }));
  },
}));
