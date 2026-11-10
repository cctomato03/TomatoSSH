import { create } from "zustand";
import type { GroupRecord, HostRecord } from "../lib/types";

interface UiState {
  hostDialog: { open: boolean; host: HostRecord | null; groupId: string | null };
  groupDialog: { open: boolean; group: GroupRecord | null; parentId: string | null };
  settingsOpen: boolean;
  syncOpen: boolean;
  openHost: (host?: HostRecord | null, groupId?: string | null) => void;
  closeHost: () => void;
  openGroup: (group?: GroupRecord | null, parentId?: string | null) => void;
  closeGroup: () => void;
  setSettingsOpen: (open: boolean) => void;
  setSyncOpen: (open: boolean) => void;
}

export const useUi = create<UiState>((set) => ({
  hostDialog: { open: false, host: null, groupId: null },
  groupDialog: { open: false, group: null, parentId: null },
  settingsOpen: false,
  syncOpen: false,
  openHost: (host = null, groupId = null) => set({ hostDialog: { open: true, host, groupId } }),
  closeHost: () => set({ hostDialog: { open: false, host: null, groupId: null } }),
  openGroup: (group = null, parentId = null) => set({ groupDialog: { open: true, group, parentId } }),
  closeGroup: () => set({ groupDialog: { open: false, group: null, parentId: null } }),
  setSettingsOpen: (open) => set({ settingsOpen: open }),
  setSyncOpen: (open) => set({ syncOpen: open }),
}));
