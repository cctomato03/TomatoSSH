import { create } from "zustand";
import { api } from "../lib/api";
import type { HostRecord, SerialProfile, TabRecord } from "../lib/types";

interface TabsState {
  tabs: TabRecord[];
  activeId: string | null;
  openSshTab: (host: HostRecord) => string;
  openSerialTab: (profile: SerialProfile) => string;
  openLocalTab: (title?: string) => string;
  closeTab: (id: string) => void;
  setActive: (id: string) => void;
  patchTab: (id: string, patch: Partial<TabRecord>) => void;
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
    };
    set((s) => ({ tabs: [...s.tabs, tab], activeId: id }));
    return id;
  },

  openSerialTab(profile) {
    const existing = get().tabs.find((t) => t.kind === "serial" && t.serialId === profile.id && t.status !== "error");
    if (existing) {
      set({ activeId: existing.id });
      return existing.id;
    }
    const id = uid();
    const tab: TabRecord = {
      id,
      kind: "serial",
      title: profile.label || `${profile.portName}@${profile.baudRate}`,
      hostId: null,
      serialId: profile.id,
      status: "connecting",
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

}));
