import { useEffect, useState } from "react";
import { Cable, FileText, Network, PlugZap, RadioTower, SlidersHorizontal, Trash2 } from "lucide-react";
import { integrations as defaultIntegrations } from "../data/production";
import { generateEasyWorshipSmbSetupGuide } from "../services/desktopApi";

import type {
  AdapterDispatchResult,
  BoothPackExport,
  CompanionConfig,
  EasyWorshipConfig,
  Integration,
  IntegrationEvent,
  ObsConfig,
  OscConfig,
  PluginVerificationResult,
  ProPresenterConfig,
  Tone,
  TrustedPlugin,
  VmixConfig,
  VmixStatus
} from "../types";
import { ActionButton, SectionHeader, StatusPill } from "./Primitives";

const categories = [
  { title: "Presentation", detail: "EasyWorship and ProPresenter handoff.", icon: FileText },
  { title: "Broadcast", detail: "OBS, vMix, NDI, and HDMI output.", icon: RadioTower },
  { title: "Automation", detail: "OSC and Companion-style control.", icon: SlidersHorizontal },
  { title: "Transport", detail: "Local HTTP, websocket, files, and display windows.", icon: Network }
];

const defaultVmixStatus: VmixStatus = {
  state: "offline",
  detail: "vMix status has not been checked yet.",
  endpoint: "http://127.0.0.1:8088/api/",
  host: "127.0.0.1",
  port: 8088,
  titleInput: "Aletheia Scripture.gtzip",
  verseField: "Headline.Text",
  referenceField: "Description.Text",
  overlayChannel: 2,
  allowPrivateNetwork: false,
  checkedAtMs: Date.now()
};

const defaultObsConfig: ObsConfig = {
  host: "127.0.0.1",
  port: 4455,
  password: "",
  sceneName: "Worship",
  sourceName: "Aletheia Scripture",
  allowPrivateNetwork: false
};

const defaultOscConfig: OscConfig = {
  host: "127.0.0.1",
  port: 9000,
  namespace: "/aletheia",
  allowPrivateNetwork: false
};

// Default reflects %LOCALAPPDATA%\Aletheia\easyworship-feed on Windows;
// the Rust backend will use the platform-appropriate path on first save.
const defaultEasyWorshipConfig: EasyWorshipConfig = {
  watchDir: "%LOCALAPPDATA%\\Aletheia\\easyworship-feed"
};

const defaultProPresenterConfig: ProPresenterConfig = {
  host: "127.0.0.1",
  port: 65002,
  messageName: "Aletheia Scripture",
  verseToken: "{{verse}}",
  referenceToken: "{{reference}}",
  allowPrivateNetwork: false
};

const defaultCompanionConfig: CompanionConfig = {
  host: "127.0.0.1",
  port: 8888,
  page: 1,
  row: 1,
  column: 1,
  verseVariable: "aletheia_verse",
  referenceVariable: "aletheia_reference",
  allowPrivateNetwork: false
};

export function IntegrationsSettings({
  integrations = defaultIntegrations,
  vmixStatus = defaultVmixStatus,
  integrationEvents = [],
  obsStatus,
  proPresenterStatus,
  companionStatus,
  oscStatus,
  easyWorshipStatus,
  operatorName = "",
  trustedPlugins = [],
  onSaveVmixConfig,
  onCheckVmix,
  onSendVmixPreview,
  onSendVmixLive,
  onClearVmix,
  onSaveObsConfig,
  onCheckObs,
  onSendObsPreview,
  onSendObsLive,
  onClearObs,
  onSaveProPresenterConfig,
  onCheckProPresenter,
  onSendProPresenterPreview,
  onSendProPresenterLive,
  onClearProPresenter,
  onSaveCompanionConfig,
  onCheckCompanion,
  onSendCompanionPreview,
  onSendCompanionLive,
  onClearCompanion,
  onSaveOscConfig,
  onCheckOsc,
  onSendOscPreview,
  onSendOscLive,
  onSendOscPing,
  onClearOsc,
  onSaveEasyWorshipConfig,
  onCheckEasyWorship,
  onSendEasyWorshipPreview,
  onSendEasyWorshipLive,
  onClearEasyWorship,
  onExportBoothPack,
  boothPackExport,
  onVerifyPluginManifest,
  onEnablePluginManifest,
  pluginVerification,
  onSaveOperatorName,
  onRevokePlugin,
}: {
  integrations?: Integration[];
  vmixStatus?: VmixStatus;
  integrationEvents?: IntegrationEvent[];
  obsStatus?: AdapterDispatchResult | null;
  proPresenterStatus?: AdapterDispatchResult | null;
  companionStatus?: AdapterDispatchResult | null;
  oscStatus?: AdapterDispatchResult | null;
  easyWorshipStatus?: AdapterDispatchResult | null;
  operatorName?: string;
  trustedPlugins?: TrustedPlugin[];
  onSaveVmixConfig?: (config: VmixConfig) => void;
  onCheckVmix?: () => void;
  onSendVmixPreview?: () => void;
  onSendVmixLive?: () => void;
  onClearVmix?: () => void;
  onSaveObsConfig?: (config: ObsConfig) => void;
  onCheckObs?: () => void;
  onSendObsPreview?: () => void;
  onSendObsLive?: () => void;
  onClearObs?: () => void;
  onSaveProPresenterConfig?: (config: ProPresenterConfig) => void;
  onCheckProPresenter?: () => void;
  onSendProPresenterPreview?: () => void;
  onSendProPresenterLive?: () => void;
  onClearProPresenter?: () => void;
  onSaveCompanionConfig?: (config: CompanionConfig) => void;
  onCheckCompanion?: () => void;
  onSendCompanionPreview?: () => void;
  onSendCompanionLive?: () => void;
  onClearCompanion?: () => void;
  onSaveOscConfig?: (config: OscConfig) => void;
  onCheckOsc?: () => void;
  onSendOscPreview?: () => void;
  onSendOscLive?: () => void;
  onSendOscPing?: () => void;
  onClearOsc?: () => void;
  onSaveEasyWorshipConfig?: (config: EasyWorshipConfig) => void;
  onCheckEasyWorship?: () => void;
  onSendEasyWorshipPreview?: () => void;
  onSendEasyWorshipLive?: () => void;
  onClearEasyWorship?: () => void;
  onExportBoothPack?: () => void;
  boothPackExport?: BoothPackExport | null;
  onVerifyPluginManifest?: (manifestPath: string, trustedKeyIds: string[]) => void;
  onEnablePluginManifest?: (manifestPath: string, trustedKeyIds: string[]) => void;
  pluginVerification?: PluginVerificationResult | null;
  onSaveOperatorName?: (name: string) => void;
  onRevokePlugin?: (pluginId: string) => void;
}) {
  const [draft, setDraft] = useState<VmixConfig>(toConfig(vmixStatus));
  const [obsDraft, setObsDraft] = useState<ObsConfig>(defaultObsConfig);
  const [ppDraft, setPpDraft] = useState<ProPresenterConfig>(defaultProPresenterConfig);
  const [companionDraft, setCompanionDraft] = useState<CompanionConfig>(defaultCompanionConfig);
  const [oscDraft, setOscDraft] = useState<OscConfig>(defaultOscConfig);
  const [ewDraft, setEwDraft] = useState<EasyWorshipConfig>(defaultEasyWorshipConfig);
  const [showEwSmbGuide, setShowEwSmbGuide] = useState(false);
  const [showVmixGtGuide, setShowVmixGtGuide] = useState(false);
  const [testVerseSent, setTestVerseSent] = useState<string | null>(null);
  const [manifestPath, setManifestPath] = useState("");
  const [trustedKeyIds, setTrustedKeyIds] = useState("");
  const [operatorDraft, setOperatorDraft] = useState(operatorName);
  const vmixTone = statusTone(vmixStatus.state);
  const obsTone = statusTone(obsStatus?.state ?? "offline");
  const ppTone = statusTone(proPresenterStatus?.state ?? "offline");
  const companionTone = statusTone(companionStatus?.state ?? "offline");
  const oscTone = statusTone(oscStatus?.state ?? "offline");
  const ewTone = statusTone(easyWorshipStatus?.state ?? "offline");

  useEffect(() => {
    setDraft(toConfig(vmixStatus));
  }, [vmixStatus]);

  return (
    <section className="space-y-7">
      <SectionHeader
        eyebrow="Integrations"
        title="Adapters stay modular and testable"
        detail="Every integration declares capabilities, credentials, dry-run support, and health. Detection never depends on a presentation vendor."
        action={<ActionButton onClick={onCheckVmix}>Check vMix</ActionButton>}
      />

      <div className="grid gap-4 md:grid-cols-4">
        {categories.map((category) => {
          const Icon = category.icon;
          return (
            <div key={category.title} className="rounded-[6px] border border-white/5 bg-white/5 p-4">
              <Icon className="h-5 w-5 text-accent" aria-hidden="true" />
              <p className="mt-3 text-sm font-semibold text-ink">{category.title}</p>
              <p className="mt-1 text-xs leading-5 text-muted">{category.detail}</p>
            </div>
          );
        })}
      </div>

      <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_420px]">
        <div className="rounded-[6px] border border-white/5 bg-white/5">
          <div className="grid grid-cols-[minmax(220px,1fr)_180px_minmax(260px,1.1fr)_160px] border-b border-white/5 px-4 py-3 text-xs font-semibold uppercase tracking-[0.12em] text-muted">
            <span>Adapter</span>
            <span>Transport</span>
            <span>Capability</span>
            <span>Status</span>
          </div>
          <div className="divide-y divide-line">
            {integrations.map((integration) => (
              <button
                key={integration.id}
                type="button"
                className="group grid w-full grid-cols-[minmax(220px,1fr)_180px_minmax(260px,1.1fr)_160px] items-center px-4 py-4 text-left transition hover:bg-mist focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
              >
                <span className="flex items-center gap-3">
                  <span className="grid h-9 w-9 place-items-center rounded-[6px] border border-white/5 bg-paper">
                    <PlugZap className="h-4 w-4 text-accent" aria-hidden="true" />
                  </span>
                  <span>
                    <span className="block text-sm font-semibold text-ink">{integration.name}</span>
                    <span className="mt-1 block text-xs text-muted">{integration.detail}</span>
                  </span>
                </span>
                <span className="text-sm text-graphite">{integration.kind}</span>
                <span className="text-sm text-muted">{integration.capability}</span>
                <span className="flex justify-end">
                  <StatusPill tone={statusTone(integration.state)} label={integration.state} />
                </span>
              </button>
            ))}
          </div>
        </div>

        <aside className="space-y-4">
          <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="text-xs font-semibold uppercase tracking-[0.16em] text-muted">vMix bridge</p>
                <h3 className="mt-2 text-xl font-semibold tracking-tight text-ink">Overlay scripture title</h3>
              </div>
              <StatusPill tone={vmixTone} label={vmixStatus.state} />
            </div>

            <p className="mt-4 text-sm leading-6 text-muted">{vmixStatus.detail}</p>

            <div className="mt-5 grid gap-3">
              <TextField label="Host" value={draft.host} onChange={(host) => setDraft((current) => ({ ...current, host }))} />
              <NumberField label="Port" value={draft.port} onChange={(port) => setDraft((current) => ({ ...current, port }))} min={1} max={65535} />
              <TextField label="Title input" value={draft.titleInput} onChange={(titleInput) => setDraft((current) => ({ ...current, titleInput }))} />
              <div className="grid grid-cols-2 gap-3">
                <TextField label="Verse field" value={draft.verseField} onChange={(verseField) => setDraft((current) => ({ ...current, verseField }))} />
                <TextField label="Reference field" value={draft.referenceField} onChange={(referenceField) => setDraft((current) => ({ ...current, referenceField }))} />
              </div>
              <div className="grid grid-cols-2 gap-3">
                <NumberField label="Overlay" value={draft.overlayChannel} onChange={(overlayChannel) => setDraft((current) => ({ ...current, overlayChannel }))} min={1} max={4} />
                <label className="flex min-h-[58px] items-center gap-3 rounded-[6px] border border-white/5 bg-paper px-3 text-sm text-graphite">
                  <input
                    type="checkbox"
                    checked={draft.allowPrivateNetwork}
                    onChange={(event) => setDraft((current) => ({ ...current, allowPrivateNetwork: event.target.checked }))}
                    className="h-4 w-4 accent-accent"
                  />
                  Private LAN
                </label>
              </div>
              <div className="grid grid-cols-2 gap-3">
                <TextField
                  label="Web user (optional)"
                  value={draft.username ?? ""}
                  onChange={(username) => setDraft((current) => ({ ...current, username }))}
                />
                <TextField
                  label="Web password"
                  value={draft.password ?? ""}
                  onChange={(password) => setDraft((current) => ({ ...current, password }))}
                />
              </div>
              <p className="text-[11px] leading-5 text-muted">
                Leave blank if vMix &gt; Settings &gt; Web Controller has &ldquo;Enhanced security&rdquo; off. Fill both fields when vMix is on a remote machine with login enforced (HTTP Basic auth).
              </p>
            </div>

            <div className="mt-5 grid grid-cols-2 gap-2 border-t border-white/5 pt-5">
              <ActionButton tone="secondary" onClick={() => onSaveVmixConfig?.(draft)}>
                Save config
              </ActionButton>
              <ActionButton tone="secondary" onClick={onCheckVmix}>
                Check
              </ActionButton>
              <ActionButton tone="secondary" onClick={onSendVmixPreview}>
                Preview title
              </ActionButton>
              <ActionButton onClick={onSendVmixLive}>Take live</ActionButton>
              <ActionButton tone="danger" onClick={onClearVmix}>
                Clear
              </ActionButton>
              <ActionButton tone="secondary" onClick={() => {
                onSendVmixPreview?.();
                setTestVerseSent("vMix");
                setTimeout(() => setTestVerseSent(null), 3000);
              }}>
                {testVerseSent === "vMix" ? "✓ Sent" : "Send test verse"}
              </ActionButton>
              <ActionButton tone="secondary" onClick={() => setShowVmixGtGuide((v) => !v)}>
                GT Title Setup
              </ActionButton>
            </div>
            {showVmixGtGuide && (
              <pre style={{
                marginTop: "14px", padding: "14px", background: "rgba(0,0,0,0.5)",
                borderRadius: "6px", fontSize: "12px", color: "#c8ffc8", whiteSpace: "pre-wrap", lineHeight: 1.6,
                border: "1px solid rgba(255,255,255,0.08)"
              }}>{[
                `# vMix GT Title Setup Guide`,
                ``,
                `The 'Title input' field in Aletheia (currently: "${draft.titleInput}") must match`,
                `the EXACT name of an input in vMix. Any title input name works — keep it consistent.`,
                ``,
                `## Option A — Use a Built-In GT Title (quickest)`,
                `1. In vMix, click Add Input → Title / XAML.`,
                `2. Choose any Lower Third template.`,
                `3. After adding, rename the input to: ${draft.titleInput}`,
                `   (Right-click thumbnail → Edit Title → change the top field.)`,
                `4. Set Verse Field to: ${draft.verseField}`,
                `5. Set Reference Field to: ${draft.referenceField}`,
                `6. Click Save config, then Check vMix.`,
                ``,
                `## Option B — Any existing GT title`,
                `1. Load your existing .gtzip into vMix.`,
                `2. Note the input name. Type it into Aletheia's 'Title input' field.`,
                `3. Note the XAML field names for verse and reference. Enter them in the fields above.`,
                `4. Save config + Check vMix.`,
                ``,
                `## Enabling vMix Remote API`,
                `1. vMix → Settings → Web Controller → tick 'Enable'.`,
                `2. Default port: 8088. Set Host in Aletheia to the vMix machine IP.`,
              ].join('\n')}</pre>
            )}
          </div>

          {/* ── OBS WebSocket v5 ── */}
          <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="text-xs font-semibold uppercase tracking-[0.16em] text-muted">OBS Studio</p>
                <h3 className="mt-2 text-xl font-semibold tracking-tight text-ink">WebSocket v5 text source</h3>
              </div>
              <StatusPill tone={obsTone} label={obsStatus?.state ?? "offline"} />
            </div>
            <p className="mt-4 text-sm leading-6 text-muted">{obsStatus?.detail ?? "OBS adapter is not yet configured."}</p>
            <div className="mt-5 grid gap-3">
              <div className="grid grid-cols-[1fr_100px] gap-3">
                <TextField label="Host" value={obsDraft.host} onChange={(host) => setObsDraft((c) => ({ ...c, host }))} />
                <NumberField label="Port" value={obsDraft.port} min={1} max={65535} onChange={(port) => setObsDraft((c) => ({ ...c, port }))} />
              </div>
              <TextField label="Password" value={obsDraft.password} onChange={(password) => setObsDraft((c) => ({ ...c, password }))} />
              <TextField label="Scene name" value={obsDraft.sceneName} onChange={(sceneName) => setObsDraft((c) => ({ ...c, sceneName }))} />
              <TextField label="Text source name" value={obsDraft.sourceName} onChange={(sourceName) => setObsDraft((c) => ({ ...c, sourceName }))} />
              <label className="flex min-h-[44px] items-center gap-3 rounded-[6px] border border-white/5 bg-paper px-3 text-sm text-graphite">
                <input
                  type="checkbox"
                  checked={obsDraft.allowPrivateNetwork}
                  onChange={(event) => setObsDraft((c) => ({ ...c, allowPrivateNetwork: event.target.checked }))}
                  className="h-4 w-4 accent-accent"
                />
                Allow private LAN
              </label>
            </div>
            <div className="mt-5 grid grid-cols-2 gap-2 border-t border-white/5 pt-5">
              <ActionButton tone="secondary" onClick={() => onSaveObsConfig?.(obsDraft)}>Save config</ActionButton>
              <ActionButton tone="secondary" onClick={onCheckObs}>Check</ActionButton>
              <ActionButton tone="secondary" onClick={onSendObsPreview}>Preview</ActionButton>
              <ActionButton onClick={onSendObsLive}>Take live</ActionButton>
              <ActionButton tone="danger" onClick={onClearObs}>Clear</ActionButton>
              <ActionButton tone="secondary" onClick={() => { onSendObsPreview?.(); setTestVerseSent("OBS"); setTimeout(() => setTestVerseSent(null), 3000); }}>
                {testVerseSent === "OBS" ? "✓ Sent" : "Send test verse"}
              </ActionButton>
            </div>
          </div>

          {/* ── OSC 1.0 UDP ── */}
          <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="text-xs font-semibold uppercase tracking-[0.16em] text-muted">OSC 1.0</p>
                <h3 className="mt-2 text-xl font-semibold tracking-tight text-ink">UDP control surface</h3>
              </div>
              <StatusPill tone={oscTone} label={oscStatus?.state ?? "offline"} />
            </div>
            <p className="mt-4 text-sm leading-6 text-muted">{oscStatus?.detail ?? "OSC adapter is not yet configured."}</p>
            <div className="mt-5 grid gap-3">
              <div className="grid grid-cols-[1fr_100px] gap-3">
                <TextField label="Host" value={oscDraft.host} onChange={(host) => setOscDraft((c) => ({ ...c, host }))} />
                <NumberField label="Port" value={oscDraft.port} min={1} max={65535} onChange={(port) => setOscDraft((c) => ({ ...c, port }))} />
              </div>
              <TextField label="Namespace" value={oscDraft.namespace} onChange={(namespace) => setOscDraft((c) => ({ ...c, namespace }))} />
              <label className="flex min-h-[44px] items-center gap-3 rounded-[6px] border border-white/5 bg-paper px-3 text-sm text-graphite">
                <input
                  type="checkbox"
                  checked={oscDraft.allowPrivateNetwork}
                  onChange={(event) => setOscDraft((c) => ({ ...c, allowPrivateNetwork: event.target.checked }))}
                  className="h-4 w-4 accent-accent"
                />
                Allow private LAN
              </label>
            </div>
            <div className="mt-5 grid grid-cols-2 gap-2 border-t border-white/5 pt-5">
              <ActionButton tone="secondary" onClick={() => onSaveOscConfig?.(oscDraft)}>Save config</ActionButton>
              <ActionButton tone="secondary" onClick={onCheckOsc}>Check</ActionButton>
              <ActionButton tone="secondary" onClick={onSendOscPreview}>Preview</ActionButton>
              <ActionButton onClick={onSendOscLive}>Take live</ActionButton>
              <ActionButton tone="secondary" onClick={onSendOscPing}>Send /ping</ActionButton>
              <ActionButton tone="danger" onClick={onClearOsc}>Clear</ActionButton>
              <ActionButton tone="secondary" onClick={() => { onSendOscPreview?.(); setTestVerseSent("OSC"); setTimeout(() => setTestVerseSent(null), 3000); }}>
                {testVerseSent === "OSC" ? "✓ Sent" : "Send test verse"}
              </ActionButton>
            </div>
          </div>

          {/* ── EasyWorship watch folder ── */}
          <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="text-xs font-semibold uppercase tracking-[0.16em] text-muted">EasyWorship 7.4+</p>
                <h3 className="mt-2 text-xl font-semibold tracking-tight text-ink">Data feed watch folder</h3>
              </div>
              <StatusPill tone={ewTone} label={easyWorshipStatus?.state ?? "offline"} />
            </div>
            <p className="mt-4 text-sm leading-6 text-muted">{easyWorshipStatus?.detail ?? "EasyWorship adapter is not yet configured."}</p>
            <div className="mt-5 grid gap-3">
              <TextField label="Watch directory" value={ewDraft.watchDir} onChange={(watchDir) => setEwDraft((c) => ({ ...c, watchDir }))} />
            </div>
            <div className="mt-5 grid grid-cols-2 gap-2 border-t border-white/5 pt-5">
              <ActionButton tone="secondary" onClick={() => onSaveEasyWorshipConfig?.(ewDraft)}>Save config</ActionButton>
              <ActionButton tone="secondary" onClick={onCheckEasyWorship}>Check folder</ActionButton>
              <ActionButton tone="secondary" onClick={onSendEasyWorshipPreview}>Preview</ActionButton>
              <ActionButton onClick={onSendEasyWorshipLive}>Take live</ActionButton>
              <ActionButton tone="danger" onClick={onClearEasyWorship}>Clear</ActionButton>
              <ActionButton tone="secondary" onClick={() => { onSendEasyWorshipPreview?.(); setTestVerseSent("EW"); setTimeout(() => setTestVerseSent(null), 3000); }}>
                {testVerseSent === "EW" ? "✓ Written" : "Send test verse"}
              </ActionButton>
              <ActionButton tone="secondary" onClick={() => setShowEwSmbGuide((v) => !v)}>
                SMB Share Setup
              </ActionButton>
            </div>
            {showEwSmbGuide && (
              <pre style={{
                marginTop: "14px", padding: "14px", background: "rgba(0,0,0,0.5)",
                borderRadius: "6px", fontSize: "12px", color: "#c8ffc8", whiteSpace: "pre-wrap", lineHeight: 1.6,
                border: "1px solid rgba(255,255,255,0.08)"
              }}>{generateEasyWorshipSmbSetupGuide(ewDraft.watchDir)}</pre>
            )}
          </div>

          {/* ── ProPresenter Stage Display ── */}
          <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="text-xs font-semibold uppercase tracking-[0.16em] text-muted">ProPresenter 7</p>
                <h3 className="mt-2 text-xl font-semibold tracking-tight text-ink">Stage display message</h3>
              </div>
              <StatusPill tone={ppTone} label={proPresenterStatus?.state ?? "offline"} />
            </div>
            <p className="mt-4 text-sm leading-6 text-muted">{proPresenterStatus?.detail ?? "ProPresenter adapter is not yet configured."}</p>
            <div className="mt-5 grid gap-3">
              <div className="grid grid-cols-[1fr_100px] gap-3">
                <TextField label="Host" value={ppDraft.host} onChange={(host) => setPpDraft((c) => ({ ...c, host }))} />
                <NumberField label="Port" value={ppDraft.port} min={1} max={65535} onChange={(port) => setPpDraft((c) => ({ ...c, port }))} />
              </div>
              <TextField label="Message name" value={ppDraft.messageName} onChange={(messageName) => setPpDraft((c) => ({ ...c, messageName }))} />
              <div className="grid grid-cols-2 gap-3">
                <TextField label="Verse token" value={ppDraft.verseToken} onChange={(verseToken) => setPpDraft((c) => ({ ...c, verseToken }))} />
                <TextField label="Reference token" value={ppDraft.referenceToken} onChange={(referenceToken) => setPpDraft((c) => ({ ...c, referenceToken }))} />
              </div>
              <label className="flex min-h-[44px] items-center gap-3 rounded-[6px] border border-white/5 bg-paper px-3 text-sm text-graphite">
                <input type="checkbox" checked={ppDraft.allowPrivateNetwork} onChange={(e) => setPpDraft((c) => ({ ...c, allowPrivateNetwork: e.target.checked }))} className="h-4 w-4 accent-accent" />
                Allow private LAN
              </label>
            </div>
            <div className="mt-5 grid grid-cols-2 gap-2 border-t border-white/5 pt-5">
              <ActionButton tone="secondary" onClick={() => onSaveProPresenterConfig?.(ppDraft)}>Save config</ActionButton>
              <ActionButton tone="secondary" onClick={onCheckProPresenter}>Check</ActionButton>
              <ActionButton tone="secondary" onClick={onSendProPresenterPreview}>Preview</ActionButton>
              <ActionButton onClick={onSendProPresenterLive}>Take live</ActionButton>
              <ActionButton tone="danger" onClick={onClearProPresenter}>Clear</ActionButton>
              <ActionButton tone="secondary" onClick={() => { onSendProPresenterPreview?.(); setTestVerseSent("PP"); setTimeout(() => setTestVerseSent(null), 3000); }}>
                {testVerseSent === "PP" ? "✓ Sent" : "Send test verse"}
              </ActionButton>
            </div>
          </div>

          {/* ── Bitfocus Companion ── */}
          <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="text-xs font-semibold uppercase tracking-[0.16em] text-muted">Bitfocus Companion</p>
                <h3 className="mt-2 text-xl font-semibold tracking-tight text-ink">HTTP variable push</h3>
              </div>
              <StatusPill tone={companionTone} label={companionStatus?.state ?? "offline"} />
            </div>
            <p className="mt-4 text-sm leading-6 text-muted">{companionStatus?.detail ?? "Companion adapter is not yet configured."}</p>
            <div className="mt-5 grid gap-3">
              <div className="grid grid-cols-[1fr_100px] gap-3">
                <TextField label="Host" value={companionDraft.host} onChange={(host) => setCompanionDraft((c) => ({ ...c, host }))} />
                <NumberField label="Port" value={companionDraft.port} min={1} max={65535} onChange={(port) => setCompanionDraft((c) => ({ ...c, port }))} />
              </div>
              <div className="grid grid-cols-3 gap-3">
                <NumberField label="Page" value={companionDraft.page} min={1} max={99} onChange={(page) => setCompanionDraft((c) => ({ ...c, page }))} />
                <NumberField label="Row" value={companionDraft.row} min={1} max={8} onChange={(row) => setCompanionDraft((c) => ({ ...c, row }))} />
                <NumberField label="Column" value={companionDraft.column} min={1} max={8} onChange={(column) => setCompanionDraft((c) => ({ ...c, column }))} />
              </div>
              <div className="grid grid-cols-2 gap-3">
                <TextField label="Verse variable" value={companionDraft.verseVariable} onChange={(verseVariable) => setCompanionDraft((c) => ({ ...c, verseVariable }))} />
                <TextField label="Ref variable" value={companionDraft.referenceVariable} onChange={(referenceVariable) => setCompanionDraft((c) => ({ ...c, referenceVariable }))} />
              </div>
              <label className="flex min-h-[44px] items-center gap-3 rounded-[6px] border border-white/5 bg-paper px-3 text-sm text-graphite">
                <input type="checkbox" checked={companionDraft.allowPrivateNetwork} onChange={(e) => setCompanionDraft((c) => ({ ...c, allowPrivateNetwork: e.target.checked }))} className="h-4 w-4 accent-accent" />
                Allow private LAN
              </label>
            </div>
            <div className="mt-5 grid grid-cols-2 gap-2 border-t border-white/5 pt-5">
              <ActionButton tone="secondary" onClick={() => onSaveCompanionConfig?.(companionDraft)}>Save config</ActionButton>
              <ActionButton tone="secondary" onClick={onCheckCompanion}>Check</ActionButton>
              <ActionButton tone="secondary" onClick={onSendCompanionPreview}>Preview</ActionButton>
              <ActionButton onClick={onSendCompanionLive}>Take live</ActionButton>
              <ActionButton tone="danger" onClick={onClearCompanion}>Clear</ActionButton>
              <ActionButton tone="secondary" onClick={() => { onSendCompanionPreview?.(); setTestVerseSent("CPN"); setTimeout(() => setTestVerseSent(null), 3000); }}>
                {testVerseSent === "CPN" ? "✓ Sent" : "Send test verse"}
              </ActionButton>
            </div>
          </div>

          {/* ── Booth compatibility pack ── */}
          <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
            <div className="flex items-start gap-3">
              <span className="grid h-10 w-10 shrink-0 place-items-center rounded-[6px] border border-white/5 bg-paper">
                <FileText className="h-4 w-4 text-accent" aria-hidden="true" />
              </span>
              <div className="min-w-0 flex-1">
                <p className="text-sm font-semibold text-ink">Booth compatibility pack</p>
                <p className="mt-2 text-sm leading-6 text-muted">
                  Writes handoff files for OBS, EasyWorship, ProPresenter, vMix, NDI, HDMI, OSC, and Companion-style controls from the current preview scripture.
                </p>
                <ActionButton className="mt-4 w-full" tone="secondary" onClick={onExportBoothPack}>
                  Export booth pack
                </ActionButton>
                {boothPackExport ? (
                  <div className="mt-4 rounded-[6px] border border-white/5 bg-paper p-3">
                    <p className="break-words font-mono text-xs text-ink">{boothPackExport.path}</p>
                    <p className="mt-2 text-xs leading-5 text-muted">
                      {boothPackExport.files.length} files ready for rehearsal, including OBS browser source, EasyWorship text handoff, and vMix setup.
                    </p>
                  </div>
                ) : null}
              </div>
            </div>
          </div>

          <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
            <p className="text-sm font-semibold text-ink">Recent delivery receipts</p>
            <div className="mt-4 space-y-3">
              {integrationEvents.length ? (
                integrationEvents.slice(0, 5).map((event) => (
                  <div key={`${event.timestampMs}-${event.action}-${event.detail}`} className="border-b border-white/5 pb-3 last:border-0 last:pb-0">
                    <div className="flex items-center justify-between gap-3">
                      <p className="text-sm font-semibold text-ink">{event.action}</p>
                      <StatusPill tone={event.severity === "error" ? "offline" : event.severity === "warn" ? "degraded" : "healthy"} label={event.severity} />
                    </div>
                    <p className="mt-1 text-xs leading-5 text-muted">{event.detail}</p>
                  </div>
                ))
              ) : (
                <p className="text-sm leading-6 text-muted">No delivery receipts yet. Use Preview title, Take live, or Clear during rehearsal.</p>
              )}
              {integrationEvents.length > 0 && (
                <button
                  type="button"
                  onClick={() => {
                    const header = "Timestamp,Integration,Action,Severity,Detail";
                    const rows = integrationEvents.map((e) =>
                      `${e.timestampMs},${e.integrationId},${JSON.stringify(e.action)},${e.severity},${JSON.stringify(e.detail)}`
                    );
                    const blob = new Blob([[header, ...rows].join("\n")], { type: "text/csv" });
                    const url = URL.createObjectURL(blob);
                    const a = document.createElement("a");
                    a.href = url;
                    a.download = `aletheia-audit-${Date.now()}.csv`;
                    a.click();
                    URL.revokeObjectURL(url);
                  }}
                  style={{
                    marginTop: 8, padding: "6px 14px", fontSize: 12,
                    background: "rgba(255,255,255,0.07)", color: "#c8ffc8",
                    border: "1px solid rgba(255,255,255,0.12)", borderRadius: 6, cursor: "pointer"
                  }}
                >
                  ⬇ Export audit log CSV
                </button>
              )}
            </div>
          </div>

          <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
            <p className="text-sm font-semibold text-ink">Plugin signature check</p>
            <p className="mt-2 text-sm leading-6 text-muted">
              Verify signed plugin manifests before enabling third-party adapters. Unsigned or policy-violating plugins are rejected.
            </p>
            <div className="mt-4 grid gap-3">
              <TextField label="Manifest path" value={manifestPath} onChange={setManifestPath} />
              <TextField
                label="Trusted key IDs (comma-separated)"
                value={trustedKeyIds}
                onChange={setTrustedKeyIds}
              />
            </div>
            <div className="mt-4 grid grid-cols-2 gap-2">
              <ActionButton
                className="w-full"
                tone="secondary"
                onClick={() =>
                  onVerifyPluginManifest?.(
                    manifestPath,
                    trustedKeyIds
                      .split(",")
                      .map((value) => value.trim())
                      .filter(Boolean)
                  )
                }
              >
                Verify manifest
              </ActionButton>
              <ActionButton
                className="w-full"
                onClick={() =>
                  onEnablePluginManifest?.(
                    manifestPath,
                    trustedKeyIds
                      .split(",")
                      .map((value) => value.trim())
                      .filter(Boolean)
                  )
                }
              >
                Verify and enable
              </ActionButton>
            </div>
            {pluginVerification ? (
              <div className="mt-4 rounded-[6px] border border-white/5 bg-paper p-3">
                <div className="flex items-center justify-between gap-2">
                  <p className="text-xs font-semibold uppercase tracking-[0.12em] text-muted">Result</p>
                  <StatusPill tone={pluginVerification.state === "rejected" ? "offline" : "healthy"} label={pluginVerification.state} />
                </div>
                <p className="mt-2 text-xs text-muted">{pluginVerification.detail}</p>
                {pluginVerification.manifest ? (
                  <p className="mt-2 font-mono text-[11px] text-graphite">
                    {pluginVerification.manifest.name} {pluginVerification.manifest.version} ·{" "}
                    {pluginVerification.manifest.capabilityCount} capabilities
                  </p>
                ) : null}
              </div>
            ) : null}
          </div>

          {/* ── Trusted plugin registry ── */}
          {trustedPlugins.length > 0 ? (
            <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
              <p className="text-sm font-semibold text-ink">Trusted plugins</p>
              <div className="mt-4 space-y-2">
                {trustedPlugins.map((plugin) => (
                  <div key={plugin.id} className="flex items-center justify-between gap-3 rounded-[6px] border border-white/5 bg-paper px-3 py-2">
                    <div className="min-w-0">
                      <p className="truncate text-sm font-semibold text-ink">{plugin.name}</p>
                      <p className="text-xs text-muted">{plugin.version} · key {plugin.keyId.slice(0, 8)}…</p>
                    </div>
                    <button
                      type="button"
                      title="Revoke plugin trust"
                      aria-label={`Revoke trust for plugin ${plugin.name}`}
                      onClick={() => onRevokePlugin?.(plugin.id)}
                      className="shrink-0 rounded p-1 text-muted hover:text-danger focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
                    >
                      <Trash2 className="h-4 w-4" aria-hidden="true" />
                    </button>
                  </div>
                ))}
              </div>
            </div>
          ) : null}

          {/* ── Operator identity ── */}
          <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
            <p className="text-sm font-semibold text-ink">Operator identity</p>
            <p className="mt-2 text-sm leading-6 text-muted">
              Shown in audit logs and support bundles. Use a descriptive name, e.g. <span className="font-mono text-xs text-graphite">booth-volunteer-1</span>.
            </p>
            <div className="mt-4 flex gap-2">
              <div className="flex-1">
                <TextField label="Operator name" value={operatorDraft} onChange={setOperatorDraft} />
              </div>
              <div className="flex items-end">
                <ActionButton tone="secondary" onClick={() => onSaveOperatorName?.(operatorDraft)}>Save</ActionButton>
              </div>
            </div>
          </div>
        </aside>
      </div>

      <div className="rounded-[6px] border border-white/5 bg-mist p-5">
        <div className="flex items-start gap-3">
          <Cable className="mt-1 h-5 w-5 text-accent" aria-hidden="true" />
          <div>
            <p className="text-sm font-semibold text-ink">Adapter policy</p>
            <p className="mt-2 text-sm leading-6 text-muted">
              Production adapters run with explicit scopes, signed manifests, dry-run checks, circuit breakers, and loopback-first control. A failed plugin degrades one destination, not the whole service.
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}

function TextField({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  return (
    <label className="block">
      <span className="text-xs font-semibold uppercase tracking-[0.12em] text-muted">{label}</span>
      <input
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="mt-1 h-10 w-full rounded-[6px] border border-white/5 bg-paper px-3 font-mono text-xs text-ink outline-none transition focus:border-accent focus:bg-white/5"
      />
    </label>
  );
}

function NumberField({ label, value, min, max, onChange }: { label: string; value: number; min: number; max: number; onChange: (value: number) => void }) {
  return (
    <label className="block">
      <span className="text-xs font-semibold uppercase tracking-[0.12em] text-muted">{label}</span>
      <input
        type="number"
        min={min}
        max={max}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
        className="mt-1 h-10 w-full rounded-[6px] border border-white/5 bg-paper px-3 font-mono text-xs text-ink outline-none transition focus:border-accent focus:bg-white/5"
      />
    </label>
  );
}

function toConfig(status: VmixStatus): VmixConfig {
  return {
    host: status.host,
    port: status.port,
    titleInput: status.titleInput,
    verseField: status.verseField,
    referenceField: status.referenceField,
    overlayChannel: status.overlayChannel,
    allowPrivateNetwork: status.allowPrivateNetwork,
    username: (status as { username?: string }).username ?? "",
    // Password never travels back from the backend; operator re-enters when changing.
    password: ""
  };
}

function statusTone(state: string): Tone {
  if (state === "degraded") return "degraded";
  if (state === "offline") return "offline";
  return "healthy";
}
