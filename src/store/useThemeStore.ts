import { create } from "zustand";
import { loadLocalSync, saveDualPersisted, loadDualPersisted } from "./persistence";

export type ThemeMode = "light" | "dark" | "system";

const STORAGE_KEY = "aletheia.themeMode";

function isThemeMode(v: unknown): v is ThemeMode {
  return v === "light" || v === "dark" || v === "system";
}

function loadInitial(): ThemeMode {
  const sync = loadLocalSync<ThemeMode>(STORAGE_KEY);
  return isThemeMode(sync) ? sync : "system";
}

interface ThemeStore {
  themeMode: ThemeMode;
  setThemeMode: (mode: ThemeMode) => void;
}

export const useThemeStore = create<ThemeStore>((set) => ({
  themeMode: loadInitial(),
  setThemeMode: (themeMode) => {
    saveDualPersisted(STORAGE_KEY, themeMode);
    set({ themeMode });
  }
}));

// Backfill Rust KV from any prior localStorage-only value, and hydrate from
// Rust KV if the operator reinstalled and brought their database across.
export async function hydrateThemeFromKv(): Promise<void> {
  const v = await loadDualPersisted<ThemeMode>(STORAGE_KEY);
  if (isThemeMode(v)) {
    useThemeStore.setState({ themeMode: v });
  }
}
