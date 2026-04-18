// Two-tier persistence helper.
//
// Each domain store calls `loadDualPersisted("aletheia.<key>")` once during
// initialisation: it tries the Rust KV first (durable across reinstalls),
// and falls back to localStorage so the UI also works in `vite dev` outside
// Tauri. Writes go to both tiers — localStorage immediately for the next
// page load, Rust KV asynchronously so the value survives even if the user
// reinstalls the app and copies their database across.
//
// The KV write is fire-and-forget (logged on failure) because the operator
// doesn't want a transient SQLite hiccup to block a UI interaction. The
// localStorage tier remains the source of truth within a session.

import { kvGet, kvSet, kvDelete } from "../services/desktopApi";

/** Returns the parsed value or `null` if neither tier has a record. */
export async function loadDualPersisted<T>(key: string): Promise<T | null> {
  // Try Rust first — it's authoritative if both tiers have data.
  try {
    const fromKv = await kvGet(key);
    if (fromKv) {
      const parsed = JSON.parse(fromKv) as T;
      // Mirror Rust → localStorage so a subsequent synchronous read sees it.
      try {
        if (typeof window !== "undefined") {
          window.localStorage.setItem(key, fromKv);
        }
      } catch {
        /* quota / private mode */
      }
      return parsed;
    }
  } catch {
    /* Rust unreachable — fall through to localStorage. */
  }

  if (typeof window === "undefined") return null;
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) return null;
    return JSON.parse(raw) as T;
  } catch {
    return null;
  }
}

/** Synchronous mirror — used during store hydration before any await. */
export function loadLocalSync<T>(key: string): T | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) return null;
    return JSON.parse(raw) as T;
  } catch {
    return null;
  }
}

/** Writes both tiers. Returns immediately after the localStorage write. */
export function saveDualPersisted<T>(key: string, value: T): void {
  const json = (() => {
    try {
      return JSON.stringify(value);
    } catch {
      return null;
    }
  })();
  if (json === null) return;

  try {
    if (typeof window !== "undefined") {
      window.localStorage.setItem(key, json);
    }
  } catch {
    /* quota */
  }
  // Fire-and-forget Rust mirror.
  void kvSet(key, json).catch(() => undefined);
}

/** Removes from both tiers. */
export function clearDualPersisted(key: string): void {
  try {
    if (typeof window !== "undefined") {
      window.localStorage.removeItem(key);
    }
  } catch {
    /* ignore */
  }
  void kvDelete(key).catch(() => undefined);
}

/** Backfills the Rust KV from any pre-existing localStorage values for the
 *  given keys. Call once on app boot so existing users don't lose state when
 *  upgrading to the dual-persist build. No-op if a Rust value already exists. */
export async function migrateLocalStorageToKv(keys: string[]): Promise<void> {
  if (typeof window === "undefined") return;
  for (const key of keys) {
    try {
      const existing = await kvGet(key);
      if (existing) continue;
      const local = window.localStorage.getItem(key);
      if (!local) continue;
      await kvSet(key, local);
    } catch {
      /* keep going — best-effort */
    }
  }
}
