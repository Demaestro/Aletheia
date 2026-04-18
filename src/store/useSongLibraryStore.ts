import { create } from "zustand";
import type { CcliUsageEntry, Song, SongSection } from "../types";
import { loadDualPersisted, loadLocalSync, saveDualPersisted } from "./persistence";

const SONGS_KEY = "aletheia.songs.v1";
const CCLI_KEY = "aletheia.ccliUsage.v1";

function now() {
  return Date.now();
}

function uid(prefix: string) {
  return `${prefix}-${Math.random().toString(36).slice(2, 10)}-${now().toString(36)}`;
}

function loadSongs(): Song[] {
  const fromLocal = loadLocalSync<Song[]>(SONGS_KEY);
  return Array.isArray(fromLocal) && fromLocal.length > 0 ? fromLocal : seedSongs();
}

function loadUsage(): CcliUsageEntry[] {
  const fromLocal = loadLocalSync<CcliUsageEntry[]>(CCLI_KEY);
  return Array.isArray(fromLocal) ? fromLocal : [];
}

function persistSongs(songs: Song[]) {
  saveDualPersisted(SONGS_KEY, songs);
}

function persistUsage(entries: CcliUsageEntry[]) {
  saveDualPersisted(CCLI_KEY, entries);
}

/** Reconciles store with Rust KV — call once on app boot. */
export async function hydrateSongLibraryFromKv(): Promise<void> {
  const songs = await loadDualPersisted<Song[]>(SONGS_KEY);
  const usage = await loadDualPersisted<CcliUsageEntry[]>(CCLI_KEY);
  const patch: Partial<{ songs: Song[]; usage: CcliUsageEntry[] }> = {};
  if (Array.isArray(songs) && songs.length > 0) patch.songs = songs;
  if (Array.isArray(usage)) patch.usage = usage;
  if (Object.keys(patch).length > 0) {
    useSongLibraryStore.setState(patch as Partial<SongLibraryState>);
  }
}

function seedSongs(): Song[] {
  const ts = now();
  return [
    {
      id: "song-amazing-grace",
      title: "Amazing Grace",
      ccliNumber: "22025",
      author: "John Newton",
      copyright: "Public Domain",
      language: "en",
      sections: [
        { label: "Verse 1", text: "Amazing grace! How sweet the sound\nThat saved a wretch like me!\nI once was lost, but now am found,\nWas blind, but now I see." },
        { label: "Verse 2", text: "'Twas grace that taught my heart to fear,\nAnd grace my fears relieved;\nHow precious did that grace appear\nThe hour I first believed!" }
      ],
      songKey: "G",
      bpm: 72,
      createdAtMs: ts,
      updatedAtMs: ts
    }
  ];
}

type SongLibraryState = {
  songs: Song[];
  usage: CcliUsageEntry[];
  activeSongId: string | null;
  activeSectionIdx: number;
  selectSong: (id: string | null) => void;
  selectSection: (idx: number) => void;
  upsertSong: (song: Song) => void;
  deleteSong: (id: string) => void;
  createSong: (partial?: Partial<Song>) => Song;
  logSectionLive: (songId: string, section: SongSection, serviceSessionId: string, operator: string) => void;
  clearUsageLog: () => void;
  exportUsageCsv: () => string;
};

export const useSongLibraryStore = create<SongLibraryState>((set, get) => ({
  songs: loadSongs(),
  usage: loadUsage(),
  activeSongId: null,
  activeSectionIdx: 0,
  selectSong: (id) => set({ activeSongId: id, activeSectionIdx: 0 }),
  selectSection: (idx) => set({ activeSectionIdx: Math.max(0, idx) }),
  upsertSong: (song) => {
    const ts = now();
    const next = { ...song, updatedAtMs: ts };
    const existing = get().songs.findIndex((s) => s.id === song.id);
    const songs = existing >= 0 ? get().songs.map((s, i) => (i === existing ? next : s)) : [...get().songs, next];
    persistSongs(songs);
    set({ songs });
  },
  deleteSong: (id) => {
    const songs = get().songs.filter((s) => s.id !== id);
    persistSongs(songs);
    set({
      songs,
      activeSongId: get().activeSongId === id ? null : get().activeSongId
    });
  },
  createSong: (partial) => {
    const ts = now();
    const song: Song = {
      id: uid("song"),
      title: partial?.title ?? "Untitled Song",
      ccliNumber: partial?.ccliNumber ?? null,
      author: partial?.author ?? "",
      copyright: partial?.copyright ?? "Public Domain",
      language: partial?.language ?? "en",
      sections: partial?.sections ?? [{ label: "Verse 1", text: "" }],
      songKey: partial?.songKey ?? null,
      bpm: partial?.bpm ?? null,
      createdAtMs: ts,
      updatedAtMs: ts
    };
    const songs = [...get().songs, song];
    persistSongs(songs);
    set({ songs, activeSongId: song.id, activeSectionIdx: 0 });
    return song;
  },
  logSectionLive: (songId, section, serviceSessionId, operator) => {
    const song = get().songs.find((s) => s.id === songId);
    if (!song || !song.ccliNumber) return;
    const entry: CcliUsageEntry = {
      id: uid("ccli"),
      ccliNumber: song.ccliNumber,
      songTitle: song.title,
      sentLiveAtMs: now(),
      serviceSessionId,
      operator
    };
    // Dedupe: ignore if the same song was logged in the last 90 seconds.
    const recent = get().usage.find(
      (u) => u.ccliNumber === entry.ccliNumber && entry.sentLiveAtMs - u.sentLiveAtMs < 90_000
    );
    if (recent) return;
    void section;
    const usage = [entry, ...get().usage].slice(0, 2000);
    persistUsage(usage);
    set({ usage });
  },
  clearUsageLog: () => {
    persistUsage([]);
    set({ usage: [] });
  },
  exportUsageCsv: () => {
    const rows = ["ccli_number,song_title,sent_live_at_iso,service_session_id,operator"];
    for (const u of get().usage) {
      const iso = new Date(u.sentLiveAtMs).toISOString();
      rows.push(
        [u.ccliNumber, u.songTitle, iso, u.serviceSessionId, u.operator]
          .map((v) => `"${String(v).replace(/"/g, '""')}"`)
          .join(",")
      );
    }
    return rows.join("\n");
  }
}));
