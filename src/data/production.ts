import {
  Activity,
  AudioLines,
  Cable,
  CheckCircle2,
  Film,
  Gauge,
  LayoutDashboard,
  MonitorUp,
  Music,
  Network,
  Palette,
  RadioTower,
  Search,
  ShieldCheck,
  Sparkles,
  Subtitles,
  Tv
} from "lucide-react";
import type {
  HardwareChecklistItem,
  HealthItem,
  Integration,
  NavItem,
  ScreenKey,
  ScriptureCandidate,
  ThemePreset,
  TranscriptSegment
} from "../types";

export const productName = "Aletheia";

// Primary operator nav — 7 items kept to what matters during a live service.
// Landing / Onboarding are accessible via the logo; Manual Search is a
// right-rail shortcut so it doesn't crowd the sidebar.
export const navItems: NavItem[] = [
  { key: "dashboard",     label: "Dashboard",    eyebrow: "Session",  icon: LayoutDashboard },
  { key: "transcript",    label: "Transcript",   eyebrow: "Listen",   icon: Subtitles },
  { key: "queue",         label: "Scripture",    eyebrow: "Approve",  icon: CheckCircle2 },
  { key: "output",        label: "Output",       eyebrow: "Present",  icon: MonitorUp },
  { key: "songs",         label: "Songs",        eyebrow: "Lyrics",   icon: Music },
  { key: "stream",        label: "Stream",       eyebrow: "Overlay",  icon: Tv },
  { key: "integrations",  label: "Integrations", eyebrow: "Connect",  icon: Cable },
  { key: "fleet",         label: "Fleet",        eyebrow: "Sync",     icon: Network },
  { key: "clips",         label: "Clips",        eyebrow: "EDL",      icon: Film },
  { key: "health",        label: "Health",       eyebrow: "Assets",   icon: Gauge },
  { key: "search",        label: "Search",       eyebrow: "Manual",   icon: Search },
];

export const heroImage =
  "https://images.unsplash.com/photo-1507692049790-de58290a4334?auto=format&fit=crop&w=2200&q=82";

export const serviceStats = [
  { label: "Service profile", value: "Sunday AM", detail: "English, Hausa, Yoruba, Twi, Swahili, Xhosa, Spanish, French" },
  { label: "Detection mode", value: "Hybrid local", detail: "Cloud enhance optional" },
  { label: "Output safety", value: "Manual live", detail: "Auto-live disabled" },
  { label: "Bandwidth", value: "Low", detail: "Metadata sync only" }
];

export const transcriptSegments: TranscriptSegment[] = [
  {
    id: "seg-1842",
    time: "00:18:42",
    speaker: "Pastor Daniel",
    language: "English",
    text: "Turn with me to Romans chapter eight. We will read verse twenty eight together.",
    confidence: 94,
    latencyMs: 410
  },
  {
    id: "seg-1847",
    time: "00:18:47",
    speaker: "Pastor Daniel",
    language: "English",
    text: "And we know that all things work together for good to them that love God.",
    confidence: 91,
    latencyMs: 438
  },
  {
    id: "seg-1855",
    time: "00:18:55",
    speaker: "Interpreter",
    language: "Yoruba",
    text: "A mo pe ohun gbogbo n ṣiṣẹ pọ fun rere fun awon ti won fe Olorun.",
    confidence: 83,
    latencyMs: 620
  },
  {
    id: "seg-1912",
    time: "00:19:12",
    speaker: "Pastor Daniel",
    language: "English",
    text: "If you are writing notes, add Isaiah forty verse thirty one for later.",
    confidence: 89,
    latencyMs: 455
  },
  {
    id: "seg-1920-ha",
    time: "00:19:20",
    speaker: "Interpreter",
    language: "Hausa",
    text: "Mu bude Romawa 8:28 tare da ikilisiya.",
    confidence: 86,
    latencyMs: 610
  },
  {
    id: "seg-1936-sw",
    time: "00:19:36",
    speaker: "Interpreter",
    language: "Swahili",
    text: "Tufungue Warumi 8:28 pamoja na kanisa.",
    confidence: 88,
    latencyMs: 590
  },
  {
    id: "seg-1952-es",
    time: "00:19:52",
    speaker: "Interpreter",
    language: "Spanish",
    text: "Abramos Romanos 8:28 juntos.",
    confidence: 90,
    latencyMs: 520
  }
];

export const scriptureCandidates: ScriptureCandidate[] = [
  {
    id: "romans-828",
    reference: "Romans 8:28",
    translation: "KJV",
    language: "English",
    text: "And we know that all things work together for good to them that love God.",
    confidence: 92,
    source: "Pastor mic",
    reason: "Exact reference plus quoted phrase in the last 12 seconds.",
    status: "preview"
  },
  {
    id: "isaiah-4031",
    reference: "Isaiah 40:31",
    translation: "KJV",
    language: "English",
    text: "They that wait upon the LORD shall renew their strength.",
    confidence: 76,
    source: "Transcript context",
    reason: "Reference was spoken, but no verse text has been quoted yet.",
    status: "new"
  },
  {
    id: "psalm-231",
    reference: "Psalm 23:1",
    translation: "KJV",
    language: "English",
    text: "The LORD is my shepherd; I shall not want.",
    confidence: 68,
    source: "Manual fallback",
    reason: "Recent service plan contains Psalm 23 and the phrase matched softly.",
    status: "approved"
  },
  {
    id: "john-316",
    reference: "John 3:16",
    translation: "KJV",
    language: "English",
    text: "For God so loved the world, that he gave his only begotten Son.",
    confidence: 64,
    source: "Phrase search",
    reason: "Phrase match only. Operator review required.",
    status: "new"
  }
];

export const integrations: Integration[] = [
  {
    id: "obs-main",
    name: "OBS Studio",
    kind: "WebSocket",
    state: "connected",
    detail: "127.0.0.1:4455 authenticated",
    capability: "Preview, live text source, clear"
  },
  {
    id: "easyworship",
    name: "EasyWorship",
    kind: "Watch folder",
    state: "ready",
    detail: "C:\\Aletheia\\EasyWorship is writable",
    capability: "Slide export, schedule handoff"
  },
  {
    id: "propresenter",
    name: "ProPresenter",
    kind: "Local API",
    state: "ready",
    detail: "192.168.1.24 allowlisted for rehearsal",
    capability: "Stage display cue, playlist handoff"
  },
  {
    id: "ndi-output",
    name: "NDI Verse Output",
    kind: "Video output",
    state: "connected",
    detail: "Aletheia Lower Third visible on network",
    capability: "Alpha lower-third feed"
  },
  {
    id: "vmix",
    name: "vMix",
    kind: "HTTP API",
    state: "degraded",
    detail: "127.0.0.1:8088, Overlay 2, Aletheia Scripture title",
    capability: "SetText fields, preview overlay, live overlay, clear"
  },
  {
    id: "hdmi-2",
    name: "HDMI 2 Projector",
    kind: "Display window",
    state: "connected",
    detail: "1920 x 1080 at 60 Hz, safe area verified",
    capability: "Fullscreen scripture output"
  },
  {
    id: "osc-automation",
    name: "OSC Automation",
    kind: "UDP",
    state: "ready",
    detail: "/aletheia/live mapped to media server",
    capability: "Cue lights, lyrics, and media scenes"
  },
  {
    id: "companion",
    name: "Companion",
    kind: "Automation profile",
    state: "ready",
    detail: "12 safe actions exported",
    capability: "Preview, live, clear, black, freeze"
  }
];

export const hardwareChecklist: HardwareChecklistItem[] = [
  {
    id: "audio-interface",
    label: "Primary audio interface",
    detail: "USB interface or mixer output feeding the pulpit mic channel.",
    required: true
  },
  {
    id: "backup-mic",
    label: "Backup microphone",
    detail: "Second mic routed to the same capture input for redundancy.",
    required: true
  },
  {
    id: "hdmi-output",
    label: "HDMI presentation output",
    detail: "Fullscreen display on projector or LED wall at 60 Hz.",
    required: true
  },
  {
    id: "ndi-output",
    label: "NDI program feed",
    detail: "NDI output visible on the production network.",
    required: false
  },
  {
    id: "vmix-or-obs",
    label: "vMix or OBS control",
    detail: "Overlay target accessible for preview and clear commands.",
    required: true
  },
  {
    id: "network-sync",
    label: "Local network sync",
    detail: "Private LAN or isolated switch for stage monitors and automation.",
    required: false
  }
];

export const healthItems: HealthItem[] = [
  {
    label: "Offline scripture library",
    state: "healthy",
    detail: "KJV, WEB, and multilingual scripture aliases are indexed locally.",
    action: "Open library"
  },
  {
    label: "STT language packs",
    state: "degraded",
    detail: "English offline ready. African language, Spanish, and French packs are queued for operator-approved install.",
    action: "Download pack"
  },
  {
    label: "Cloud enhancement",
    state: "offline",
    detail: "Internet unstable. Local matcher is active.",
    action: "Stay offline"
  },
  {
    label: "Sync backlog",
    state: "degraded",
    detail: "42 service events waiting for low-bandwidth sync.",
    action: "Review queue"
  },
  {
    label: "Disk reserve",
    state: "healthy",
    detail: "18.4 GB available for models, logs, and backups.",
    action: "Run cleanup"
  }
];

export const themes: ThemePreset[] = [
  {
    id: "broadcast-lower",
    name: "Broadcast Lower Third",
    mode: "Lower third",
    contrast: "AA passed",
    fontScale: "42 px at 1080p",
    languages: ["English", "Yoruba", "Hausa", "Swahili"]
  },
  {
    id: "projector-full",
    name: "Projector Full Scripture",
    mode: "Full screen",
    contrast: "AAA passed",
    fontScale: "68 px at 1080p",
    languages: ["English", "Igbo", "Spanish", "French"]
  },
  {
    id: "stage-reader",
    name: "Stage Reader",
    mode: "Stage display",
    contrast: "AA passed",
    fontScale: "74 px at 1080p",
    languages: ["English", "Twi", "Xhosa"]
  }
];

export const onboardingSteps = [
  {
    title: "Install local scripture library",
    status: "Complete",
    detail: "KJV, WEB, and multilingual reference aliases indexed for offline search."
  },
  {
    title: "Select pulpit audio source",
    status: "Complete",
    detail: "Focusrite USB input detected with stable level."
  },
  {
    title: "Connect presentation destination",
    status: "Needs check",
    detail: "OBS is ready. HDMI 2 should be tested before rehearsal."
  },
  {
    title: "Rehearse preview before live",
    status: "Next",
    detail: "Send Romans 8:28 to Preview, then clear without touching Live."
  }
];

export const manualSearchResults = [
  {
    reference: "John 3:16",
    translation: "KJV",
    snippet: "For God so loved the world, that he gave his only begotten Son.",
    source: "Exact reference"
  },
  {
    reference: "1 John 3:16",
    translation: "KJV",
    snippet: "Hereby perceive we the love of God, because he laid down his life for us.",
    source: "Similar reference"
  },
  {
    reference: "Psalm 23:1",
    translation: "KJV",
    snippet: "The LORD is my shepherd; I shall not want.",
    source: "Recent service plan"
  }
];

export const screenOrder: ScreenKey[] = [
  "landing",
  "dashboard",
  "transcript",
  "queue",
  "output",
  "theme",
  "integrations",
  "health",
  "onboarding",
  "search"
];
