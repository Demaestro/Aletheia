import { create } from "zustand";
import type { StreamOverlayState } from "../types";
import { loadDualPersisted, loadLocalSync, saveDualPersisted } from "./persistence";

const STORAGE_KEY = "aletheia.streamOverlay.v1";

type StreamOverlayActions = {
  setTickerText: (text: string) => void;
  setArmed: (armed: boolean) => void;
  publishLive: (reference: string) => void;
  clearLive: () => void;
  setBrowserSourcePath: (path: string | null) => void;
  exportHtml: () => string;
};

const BASE: StreamOverlayState = {
  tickerText: "Welcome — we're glad you're here.",
  armed: false,
  liveReference: null,
  browserSourcePath: null
};

function loadInitial(): StreamOverlayState {
  const parsed = loadLocalSync<Partial<StreamOverlayState>>(STORAGE_KEY);
  if (!parsed) return BASE;
  return {
    tickerText: typeof parsed.tickerText === "string" ? parsed.tickerText : BASE.tickerText,
    armed: Boolean(parsed.armed),
    liveReference: typeof parsed.liveReference === "string" ? parsed.liveReference : null,
    browserSourcePath: typeof parsed.browserSourcePath === "string" ? parsed.browserSourcePath : null
  };
}

function persist(state: StreamOverlayState) {
  saveDualPersisted(STORAGE_KEY, {
    tickerText: state.tickerText,
    armed: state.armed,
    liveReference: state.liveReference,
    browserSourcePath: state.browserSourcePath
  });
}

/** Reconciles store with Rust KV — call once on app boot. */
export async function hydrateStreamOverlayFromKv(): Promise<void> {
  const parsed = await loadDualPersisted<Partial<StreamOverlayState>>(STORAGE_KEY);
  if (!parsed) return;
  useStreamOverlayStore.setState({
    tickerText: typeof parsed.tickerText === "string" ? parsed.tickerText : BASE.tickerText,
    armed: Boolean(parsed.armed),
    liveReference: typeof parsed.liveReference === "string" ? parsed.liveReference : null,
    browserSourcePath: typeof parsed.browserSourcePath === "string" ? parsed.browserSourcePath : null
  });
}

function buildHtml(state: StreamOverlayState): string {
  const ref = state.liveReference ? escape(state.liveReference) : "";
  const ticker = escape(state.tickerText);
  return `<!doctype html>
<html><head><meta charset="utf-8"/><title>Aletheia Stream Overlay</title>
<style>
  :root { color-scheme: dark; }
  html,body { margin:0; padding:0; background:transparent; font-family:Inter,system-ui,sans-serif; color:#fff; }
  .wrap { position:fixed; inset:0; pointer-events:none; }
  .ref { position:absolute; left:4%; bottom:14%; padding:10px 18px; border-radius:6px;
    background:linear-gradient(90deg,rgba(124,58,237,.85),rgba(79,70,229,.85));
    font-size:28px; font-weight:600; letter-spacing:.01em;
    box-shadow:0 8px 28px -8px rgba(124,58,237,.6); }
  .ticker { position:absolute; left:0; right:0; bottom:0; overflow:hidden;
    background:rgba(10,10,14,.72); padding:8px 0; font-size:16px; white-space:nowrap; }
  .ticker > span { display:inline-block; padding-left:100%; animation:marq 28s linear infinite; }
  @keyframes marq { 0%{transform:translateX(0)} 100%{transform:translateX(-100%)} }
</style></head><body>
<div class="wrap">
  ${ref ? `<div class="ref">${ref}</div>` : ""}
  <div class="ticker"><span>${ticker}</span></div>
</div></body></html>`;
}

function escape(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c] ?? c));
}

export const useStreamOverlayStore = create<StreamOverlayState & StreamOverlayActions>((set, get) => ({
  ...loadInitial(),
  setTickerText: (text) => {
    const next = { ...get(), tickerText: text };
    persist(next);
    set({ tickerText: text });
  },
  setArmed: (armed) => {
    const next = { ...get(), armed };
    persist(next);
    set({ armed });
  },
  publishLive: (reference) => {
    const { armed } = get();
    if (!armed) return;
    const next = { ...get(), liveReference: reference };
    persist(next);
    set({ liveReference: reference });
  },
  clearLive: () => {
    const next = { ...get(), liveReference: null };
    persist(next);
    set({ liveReference: null });
  },
  setBrowserSourcePath: (path) => {
    const next = { ...get(), browserSourcePath: path };
    persist(next);
    set({ browserSourcePath: path });
  },
  exportHtml: () => buildHtml(get())
}));
