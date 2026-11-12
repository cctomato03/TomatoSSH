import { create } from "zustand";
import { api } from "../lib/api";
import { applyThemeVars, getTheme, DEFAULT_THEME_ID } from "../lib/themes";
import type {
  AppSettings,
  GroupRecord,
  HostDraft,
  HostRecord,
  SerialProfile,
  Snippet,
} from "../lib/types";

export const DEFAULT_SETTINGS: AppSettings = {
  theme: DEFAULT_THEME_ID,
  fontFamily: "JetBrains Mono, Fira Code, Cascadia Code, Menlo, Consolas, monospace",
  fontSize: 14,
  lineHeight: 1.25,
  cursorBlink: true,
  scrollback: 8000,
  copyOnSelect: false,
  serialLogDir: "",
  serialLogMode: "raw",
  sync: { serverUrl: "", email: "", autoSync: false, lastSyncAt: null },
};

interface VaultState {
  loaded: boolean;
  loading: boolean;
  error: string | null;
  hosts: HostRecord[];
  groups: GroupRecord[];
  serials: SerialProfile[];
  snippets: Snippet[];
  settings: AppSettings;
  load: () => Promise<void>;
  saveHost: (draft: HostDraft) => Promise<HostRecord>;
  deleteHost: (id: string) => Promise<void>;
  saveGroup: (draft: Partial<GroupRecord> & { name: string }) => Promise<GroupRecord>;
  deleteGroup: (id: string) => Promise<void>;
  saveSerial: (draft: SerialProfile) => Promise<SerialProfile>;
  deleteSerial: (id: string) => Promise<void>;
  saveSnippet: (draft: Snippet) => Promise<Snippet>;
  deleteSnippet: (id: string) => Promise<void>;
  updateSettings: (patch: Partial<AppSettings>) => void;
  /** 云同步后重新拉取整库 */
  refreshFromServer: () => Promise<void>;
}

let persistTimer: number | undefined;

export const useVault = create<VaultState>((set, get) => ({
  loaded: false,
  loading: false,
  error: null,
  hosts: [],
  groups: [],
  serials: [],
  snippets: [],
  settings: DEFAULT_SETTINGS,

  async load() {
    set({ loading: true, error: null });
    try {
      const snap = await api.loadVault();
      const settings: AppSettings = {
        ...DEFAULT_SETTINGS,
        ...snap.settings,
        sync: { ...DEFAULT_SETTINGS.sync, ...(snap.settings?.sync ?? {}) },
      };
      applyThemeVars(getTheme(settings.theme));
      set({
        hosts: snap.hosts,
        groups: snap.groups,
        serials: snap.serials ?? [],
        snippets: snap.snippets ?? [],
        settings,
        loaded: true,
        loading: false,
      });
    } catch (e) {
      set({ error: String(e), loading: false, loaded: true });
    }
  },

  async saveHost(draft) {
    const saved = await api.hostSave(draft);
    set((s) => {
      const idx = s.hosts.findIndex((h) => h.id === saved.id);
      const hosts = idx >= 0 ? s.hosts.map((h) => (h.id === saved.id ? saved : h)) : [...s.hosts, saved];
      return { hosts };
    });
    return saved;
  },

  async deleteHost(id) {
    await api.hostDelete(id);
    set((s) => ({ hosts: s.hosts.filter((h) => h.id !== id) }));
  },

  async saveGroup(draft) {
    const saved = await api.groupSave(draft);
    set((s) => {
      const idx = s.groups.findIndex((g) => g.id === saved.id);
      const groups = idx >= 0 ? s.groups.map((g) => (g.id === saved.id ? saved : g)) : [...s.groups, saved];
      return { groups };
    });
    return saved;
  },

  async deleteGroup(id) {
    await api.groupDelete(id);
    // 删除分组后，其子分组与主机被上移到根
    set((s) => ({
      groups: s.groups
        .filter((g) => g.id !== id)
        .map((g) => (g.parentId === id ? { ...g, parentId: null } : g)),
      hosts: s.hosts.map((h) => (h.groupId === id ? { ...h, groupId: null } : h)),
    }));
  },

  updateSettings(patch) {
    const next: AppSettings = { ...get().settings, ...patch };
    set({ settings: next });
    if (patch.theme) applyThemeVars(getTheme(next.theme));
    window.clearTimeout(persistTimer);
    persistTimer = window.setTimeout(() => {
      api.settingsSave(next).catch((e) => console.error("保存设置失败", e));
    }, 250);
  },

  async saveSerial(draft) {
    const saved = await api.serialSave(draft);
    set((s) => {
      const idx = s.serials.findIndex((x) => x.id === saved.id);
      const serials = idx >= 0 ? s.serials.map((x) => (x.id === saved.id ? saved : x)) : [...s.serials, saved];
      return { serials };
    });
    return saved;
  },

  async deleteSerial(id) {
    await api.serialDelete(id);
    set((s) => ({ serials: s.serials.filter((x) => x.id !== id) }));
  },

  async saveSnippet(draft) {
    const saved = await api.snippetSave(draft);
    set((s) => {
      const idx = s.snippets.findIndex((x) => x.id === saved.id);
      const snippets =
        idx >= 0 ? s.snippets.map((x) => (x.id === saved.id ? saved : x)) : [...s.snippets, saved];
      return { snippets };
    });
    return saved;
  },

  async deleteSnippet(id) {
    await api.snippetDelete(id);
    set((s) => ({ snippets: s.snippets.filter((x) => x.id !== id) }));
  },

  async refreshFromServer() {
    const snap = await api.loadVault();
    const settings: AppSettings = {
      ...DEFAULT_SETTINGS,
      ...snap.settings,
      sync: { ...DEFAULT_SETTINGS.sync, ...(snap.settings?.sync ?? {}) },
    };
    applyThemeVars(getTheme(settings.theme));
    set({
      hosts: snap.hosts,
      groups: snap.groups,
      serials: snap.serials ?? [],
      snippets: snap.snippets ?? [],
      settings,
    });
  },
}));
