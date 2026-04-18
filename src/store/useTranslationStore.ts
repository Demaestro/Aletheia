import { create } from "zustand";
import { loadDualPersisted, saveDualPersisted } from "./persistence";

const STORAGE_KEY = "aletheia.translation.v1";

export type TranslationAdapterKind = "none" | "echo" | "http";

/** A single glossary entry — phrase mapped to its preferred translation
 *  (or transliteration). The matcher is case-insensitive whole-word, so
 *  "Yahweh" → "Yahwé" applies even mid-sentence. Glossary lookups happen
 *  before the adapter is called, so they hold even when offline. */
export type GlossaryEntry = {
  /** Source language code; if blank we apply the entry to every source. */
  source: string;
  /** Target language code. */
  target: string;
  /** Word or short phrase to match in the source text. */
  term: string;
  /** Replacement to insert into the translated output. */
  replacement: string;
};

export type TranslationSettings = {
  adapter: TranslationAdapterKind;
  httpEndpoint: string;
  httpApiKey: string;
  sourceLanguage: string;
  targetLanguages: string[];
  enabledForStream: boolean;
  enabledForRoom: boolean;
  /** When true, translations from the AI-detection result auto-populate
   *  the active target list — operators don't have to pick by hand. */
  autoRouteFromDetection: boolean;
  /** User-curated glossary of theological / proper-noun overrides. */
  glossary: GlossaryEntry[];
};

type State = TranslationSettings & {
  setAdapter: (a: TranslationAdapterKind) => void;
  setHttpEndpoint: (v: string) => void;
  setHttpApiKey: (v: string) => void;
  setSourceLanguage: (v: string) => void;
  setTargetLanguages: (v: string[]) => void;
  setEnabledForStream: (v: boolean) => void;
  setEnabledForRoom: (v: boolean) => void;
  setAutoRouteFromDetection: (v: boolean) => void;
  addGlossaryEntry: (entry: GlossaryEntry) => void;
  removeGlossaryEntry: (idx: number) => void;
  applyDetectedLanguages: (codes: string[]) => void;
  translate: (text: string, target: string) => Promise<string>;
};

const DEFAULTS: TranslationSettings = {
  adapter: "none",
  httpEndpoint: "",
  httpApiKey: "",
  sourceLanguage: "en",
  targetLanguages: [],
  enabledForStream: false,
  enabledForRoom: false,
  autoRouteFromDetection: false,
  glossary: []
};

// In-memory cache for HTTP translation results. Keyed by `source|target|text`.
// Bounded to MAX_CACHE entries; oldest evicted on overflow. Cache is
// intentionally NOT persisted — it's a perf nicety, not a source of truth.
const MAX_CACHE = 256;
const httpCache = new Map<string, string>();
function cacheGet(key: string): string | undefined {
  return httpCache.get(key);
}
function cachePut(key: string, value: string): void {
  if (httpCache.size >= MAX_CACHE) {
    const firstKey = httpCache.keys().next().value;
    if (firstKey !== undefined) httpCache.delete(firstKey);
  }
  httpCache.set(key, value);
}

/** Apply glossary substitutions to a translated string, post-adapter.
 *  Uses word-boundary matching that is friendly to non-ASCII scripts. */
function applyGlossary(
  text: string,
  glossary: GlossaryEntry[],
  source: string,
  target: string
): string {
  let out = text;
  for (const entry of glossary) {
    if (entry.target !== target) continue;
    if (entry.source && entry.source !== source) continue;
    if (!entry.term) continue;
    const escaped = entry.term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    // \b doesn't work for non-Latin scripts, so we use lookarounds.
    const re = new RegExp(`(?<![\\p{L}\\p{N}])${escaped}(?![\\p{L}\\p{N}])`, "giu");
    out = out.replace(re, entry.replacement);
  }
  return out;
}

/** fetch with timeout + exponential backoff retry. */
async function fetchWithRetry(
  url: string,
  init: RequestInit,
  attempts = 3,
  timeoutMs = 5000
): Promise<Response> {
  let lastErr: unknown;
  for (let attempt = 0; attempt < attempts; attempt++) {
    const ac = new AbortController();
    const timer = setTimeout(() => ac.abort(), timeoutMs);
    try {
      const res = await fetch(url, { ...init, signal: ac.signal });
      clearTimeout(timer);
      // Retry on 5xx + 429; otherwise return as-is.
      if (res.status >= 500 || res.status === 429) {
        lastErr = new Error(`HTTP ${res.status}`);
      } else {
        return res;
      }
    } catch (err) {
      clearTimeout(timer);
      lastErr = err;
    }
    // Backoff: 200ms, 600ms, 1.4s
    const delay = 200 * Math.pow(3, attempt);
    await new Promise((r) => setTimeout(r, delay));
  }
  throw lastErr instanceof Error ? lastErr : new Error("translate request failed");
}

function load(): TranslationSettings {
  if (typeof window === "undefined") return DEFAULTS;
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULTS;
    const parsed = JSON.parse(raw) as Partial<TranslationSettings>;
    return { ...DEFAULTS, ...parsed };
  } catch {
    return DEFAULTS;
  }
}

function persist(s: TranslationSettings) {
  saveDualPersisted(STORAGE_KEY, s);
}

/** Reconciles store with Rust KV — call once on app boot. */
export async function hydrateTranslationFromKv(): Promise<void> {
  const parsed = await loadDualPersisted<Partial<TranslationSettings>>(STORAGE_KEY);
  if (!parsed) return;
  useTranslationStore.setState({ ...DEFAULTS, ...parsed });
}

export const useTranslationStore = create<State>((set, get) => ({
  ...load(),
  setAdapter: (a) => {
    const s = { ...get(), adapter: a };
    persist(snapshot(s));
    set({ adapter: a });
  },
  setHttpEndpoint: (v) => {
    const s = { ...get(), httpEndpoint: v };
    persist(snapshot(s));
    set({ httpEndpoint: v });
  },
  setHttpApiKey: (v) => {
    const s = { ...get(), httpApiKey: v };
    persist(snapshot(s));
    set({ httpApiKey: v });
  },
  setSourceLanguage: (v) => {
    const s = { ...get(), sourceLanguage: v };
    persist(snapshot(s));
    set({ sourceLanguage: v });
  },
  setTargetLanguages: (v) => {
    const s = { ...get(), targetLanguages: v };
    persist(snapshot(s));
    set({ targetLanguages: v });
  },
  setEnabledForStream: (v) => {
    const s = { ...get(), enabledForStream: v };
    persist(snapshot(s));
    set({ enabledForStream: v });
  },
  setEnabledForRoom: (v) => {
    const s = { ...get(), enabledForRoom: v };
    persist(snapshot(s));
    set({ enabledForRoom: v });
  },
  setAutoRouteFromDetection: (v) => {
    const s = { ...get(), autoRouteFromDetection: v };
    persist(snapshot(s));
    set({ autoRouteFromDetection: v });
  },
  addGlossaryEntry: (entry) => {
    const next = [...get().glossary, entry];
    persist(snapshot({ ...get(), glossary: next }));
    set({ glossary: next });
  },
  removeGlossaryEntry: (idx) => {
    const next = get().glossary.filter((_, i) => i !== idx);
    persist(snapshot({ ...get(), glossary: next }));
    set({ glossary: next });
  },
  applyDetectedLanguages: (codes) => {
    const { autoRouteFromDetection, sourceLanguage, targetLanguages } = get();
    if (!autoRouteFromDetection) return;
    const filtered = codes
      .map((c) => c.toLowerCase().slice(0, 5))
      .filter((c) => c && c !== sourceLanguage);
    const merged = Array.from(new Set([...targetLanguages, ...filtered]));
    if (merged.length === targetLanguages.length) return;
    persist(snapshot({ ...get(), targetLanguages: merged }));
    set({ targetLanguages: merged });
  },
  translate: async (text, target) => {
    const { adapter, httpEndpoint, httpApiKey, sourceLanguage, glossary } = get();
    if (!text || target === sourceLanguage) return text;
    if (adapter === "none") return text;
    if (adapter === "echo") {
      return applyGlossary(`[${target}] ${text}`, glossary, sourceLanguage, target);
    }
    if (!httpEndpoint) return text;
    const cacheKey = `${sourceLanguage}|${target}|${text}`;
    const cached = cacheGet(cacheKey);
    if (cached !== undefined) return applyGlossary(cached, glossary, sourceLanguage, target);
    try {
      const resp = await fetchWithRetry(httpEndpoint, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          ...(httpApiKey ? { Authorization: `Bearer ${httpApiKey}` } : {})
        },
        body: JSON.stringify({ text, source: sourceLanguage, target })
      });
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
      const data = (await resp.json()) as { text?: string };
      const out = data.text ?? text;
      cachePut(cacheKey, out);
      return applyGlossary(out, glossary, sourceLanguage, target);
    } catch (err) {
      console.warn("translate http adapter failed", err);
      return text;
    }
  }
}));

function snapshot(s: TranslationSettings): TranslationSettings {
  return {
    adapter: s.adapter,
    httpEndpoint: s.httpEndpoint,
    httpApiKey: s.httpApiKey,
    sourceLanguage: s.sourceLanguage,
    targetLanguages: s.targetLanguages,
    enabledForStream: s.enabledForStream,
    enabledForRoom: s.enabledForRoom,
    autoRouteFromDetection: s.autoRouteFromDetection,
    glossary: s.glossary
  };
}
