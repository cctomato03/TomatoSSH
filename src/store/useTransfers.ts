import { create } from "zustand";
import type { TransferProgress } from "../lib/types";

interface TransfersState {
  items: TransferProgress[];
  upsert: (p: TransferProgress) => void;
  remove: (id: string) => void;
  clearFinished: () => void;
}

export const useTransfers = create<TransfersState>((set) => ({
  items: [],
  upsert: (p) =>
    set((s) => {
      const idx = s.items.findIndex((i) => i.id === p.id);
      if (idx >= 0) {
        const items = [...s.items];
        items[idx] = p;
        return { items };
      }
      return { items: [...s.items, p] };
    }),
  remove: (id) => set((s) => ({ items: s.items.filter((i) => i.id !== id) })),
  clearFinished: () => set((s) => ({ items: s.items.filter((i) => i.status === "running") })),
}));
