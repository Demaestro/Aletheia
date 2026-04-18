import { useState } from "react";
import {
  Activity,
  Archive,
  Database,
  Download,
  HardDrive,
  KeyRound,
  PackageCheck,
  ShieldCheck,
  WifiOff,
  type LucideIcon
} from "lucide-react";
import { healthItems } from "../data/production";
import type { HealthItem, LocalRehearsalReport, OfflinePackExport, ProductionReadinessReport, SupportBundleExport, Tone } from "../types";
import { ActionButton, SectionHeader, StatusPill } from "./Primitives";

const fallbackReadiness: ProductionReadinessReport = {
  generatedAtMs: Date.now(),
  score: 0,
  state: "degraded",
  blockers: ["Production readiness has not been checked yet."],
  secretVault: {
    provider: "OS vault boundary",
    state: "ready",
    detail: "Secret vault status has not been loaded yet.",
    storedSecretCount: 0,
    releaseRequired: true,
    policy: []
  },
  pluginPolicy: {
    state: "blocked",
    detail: "Plugin signing policy has not been loaded yet.",
    trustedKeyCount: 0,
    requiredControls: []
  },
  supportBundle: {
    state: "ready",
    detail: "Support bundle policy has not been loaded yet.",
    includes: [],
    excludes: []
  },
  offlineAssets: {
    state: "degraded",
    installedCount: 0,
    requiredCount: 0,
    assets: []
  },
  acceptanceDevices: [],
  releaseGates: []
};

export function HealthStatusPanel({
  items = healthItems,
  readiness = fallbackReadiness,
  supportBundleExport,
  rehearsalReport,
  offlinePackExport,
  onRunCheck,
  onRunLocalRehearsal,
  onExportSupportBundle,
  onExportOfflinePack,
  onInstallOfflineAsset,
  onInstallOfflineAssetFromPath,
  onRecordDeviceAcceptance
}: {
  items?: HealthItem[];
  readiness?: ProductionReadinessReport;
  supportBundleExport?: SupportBundleExport | null;
  rehearsalReport?: LocalRehearsalReport | null;
  offlinePackExport?: OfflinePackExport | null;
  onRunCheck?: () => void;
  onRunLocalRehearsal?: () => void;
  onExportSupportBundle?: () => void;
  onExportOfflinePack?: (targetDir: string) => void;
  onInstallOfflineAsset?: (assetId: string) => void;
  onInstallOfflineAssetFromPath?: (assetId: string, filePath: string, expectedChecksum: string) => void;
  onRecordDeviceAcceptance?: (
    deviceId: string,
    stepLabel: string,
    passed: boolean,
    note?: string,
    evidencePath?: string
  ) => void;
}) {
  const requiredDeviceCount = readiness.acceptanceDevices.filter((device) => device.requiredForRelease).length;
  const [assetPaths, setAssetPaths] = useState<Record<string, string>>({});
  const [assetChecksums, setAssetChecksums] = useState<Record<string, string>>({});
  const [deviceNotes, setDeviceNotes] = useState<Record<string, string>>({});
  const [deviceEvidence, setDeviceEvidence] = useState<Record<string, string>>({});
  const [offlinePackPath, setOfflinePackPath] = useState("C:\\Aletheia\\offline-pack");

  return (
    <section className="space-y-7">
      <SectionHeader
        eyebrow="Offline and health"
        title="Know what still works before the service starts"
        detail="The health panel prioritizes blocking issues first and translates technical failures into operator actions."
        action={
          <div className="flex flex-wrap gap-2">
            <ActionButton tone="secondary" onClick={onRunLocalRehearsal}>
              Run local rehearsal
            </ActionButton>
            <ActionButton onClick={onRunCheck}>Run pre-service check</ActionButton>
          </div>
        }
      />

      <div className="grid gap-5 xl:grid-cols-[340px_minmax(0,1fr)]">
        <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
          <p className="text-xs font-semibold uppercase tracking-[0.16em] text-muted">Offline readiness</p>
          <p className="mt-3 text-5xl font-semibold tracking-tight text-ink">{readiness.score}%</p>
          <p className="mt-3 text-sm leading-6 text-muted">
            {readiness.score >= 90
              ? "System is ready for live service. All critical offline assets are installed."
              : readiness.score >= 70
              ? "Ready for English text detection. Some offline STT packs need installation for full language coverage."
              : "Pre-service check required. Run readiness check to identify and resolve blocking issues."}
          </p>
          <div className="mt-5 space-y-3">
            <Readiness icon={Database} label="Library" value="Ready" />
            <Readiness icon={Activity} label="Detection" value="Local active" />
            <Readiness icon={WifiOff} label="Internet" value="Not required" />
            <Readiness icon={HardDrive} label="Storage" value="18.4 GB" />
          </div>
        </div>

        <div className="rounded-[6px] border border-white/5 bg-white/5">
          <div className="grid grid-cols-[minmax(220px,1fr)_140px_minmax(260px,1fr)_150px] border-b border-white/5 px-4 py-3 text-xs font-semibold uppercase tracking-[0.12em] text-muted">
            <span>System</span>
            <span>State</span>
            <span>Detail</span>
            <span>Action</span>
          </div>
          <div className="divide-y divide-line">
            {items.map((item) => (
              <div
                key={item.label}
                className="grid grid-cols-[minmax(220px,1fr)_140px_minmax(260px,1fr)_150px] items-center px-4 py-4"
              >
                <p className="text-sm font-semibold text-ink">{item.label}</p>
                <StatusPill tone={item.state} label={item.state} />
                <p className="text-sm leading-6 text-muted">{item.detail}</p>
                <button
                  type="button"
                  className="justify-self-start rounded-[6px] border border-white/5 bg-white/5 px-3 py-2 text-xs font-semibold text-ink transition hover:border-accent focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
                >
                  {item.action}
                </button>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="rounded-[6px] border border-white/5 bg-white/5">
        <div className="grid grid-cols-[minmax(180px,0.8fr)_120px_minmax(260px,1fr)_180px] border-b border-white/5 px-4 py-3 text-xs font-semibold uppercase tracking-[0.12em] text-muted">
          <span>Device</span>
          <span>State</span>
          <span>Steps</span>
          <span>Actions</span>
        </div>
        <div className="divide-y divide-line">
          {readiness.acceptanceDevices.map((device) => (
            <div key={device.id} className="px-4 py-4">
              <div className="grid grid-cols-[minmax(180px,0.8fr)_120px_minmax(260px,1fr)_180px] items-center">
                <div>
                  <p className="text-sm font-semibold text-ink">{device.name}</p>
                  <p className="text-xs text-muted">{device.category}</p>
                </div>
                <StatusPill tone={stateTone(device.state)} label={device.state} />
                <div className="space-y-3">
                  {device.steps.map((step) => {
                    const key = `${device.id}:${step.label}`;
                    return (
                      <div key={step.label} className="space-y-2 rounded-[6px] border border-white/5 bg-paper p-3">
                        <div className="flex items-center justify-between gap-3">
                          <div>
                            <p className="text-xs font-semibold text-ink">{step.label}</p>
                            <p className="text-[11px] text-muted">{step.expected}</p>
                          </div>
                          <span className="text-[11px] text-muted">{step.required ? "Required" : "Optional"}</span>
                        </div>
                        <div className="grid gap-2">
                          <input
                            value={deviceNotes[key] ?? ""}
                            onChange={(event) => setDeviceNotes((current) => ({ ...current, [key]: event.target.value }))}
                            placeholder="Note (optional)"
                            className="h-8 rounded-[6px] border border-white/5 px-3 text-xs text-ink placeholder:text-muted focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
                          />
                          <input
                            value={deviceEvidence[key] ?? ""}
                            onChange={(event) => setDeviceEvidence((current) => ({ ...current, [key]: event.target.value }))}
                            placeholder="Evidence path (optional)"
                            className="h-8 rounded-[6px] border border-white/5 px-3 text-xs text-ink placeholder:text-muted focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
                          />
                        </div>
                        <div className="flex flex-wrap gap-2">
                          <button
                            type="button"
                            className="rounded-[6px] border border-white/5 bg-white/5 px-3 py-1.5 text-[11px] font-semibold uppercase tracking-[0.12em] text-ink transition hover:border-accent/40 hover:text-accent"
                            onClick={() =>
                              onRecordDeviceAcceptance?.(
                                device.id,
                                step.label,
                                true,
                                deviceNotes[key],
                                deviceEvidence[key]
                              )
                            }
                          >
                            Mark pass
                          </button>
                          <button
                            type="button"
                            className="rounded-[6px] border border-white/5 bg-white/5 px-3 py-1.5 text-[11px] font-semibold uppercase tracking-[0.12em] text-ink transition hover:border-accent/40 hover:text-accent"
                            onClick={() =>
                              onRecordDeviceAcceptance?.(
                                device.id,
                                step.label,
                                false,
                                deviceNotes[key],
                                deviceEvidence[key]
                              )
                            }
                          >
                            Mark fail
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>
                <div className="text-xs text-muted">
                  {device.requiredForRelease ? "Required for release" : "Optional"}
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="rounded-[6px] border border-white/5 bg-white/5">
        <div className="flex flex-col gap-4 border-b border-white/5 px-5 py-4 md:flex-row md:items-center md:justify-between">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.16em] text-muted">Offline distribution pack</p>
            <p className="mt-2 text-sm leading-6 text-muted">
              Builds a USB-ready pack with installed assets, checksums, and a manifest for air-gapped installs.
            </p>
          </div>
          <ActionButton tone="secondary" onClick={() => onExportOfflinePack?.(offlinePackPath)}>
            Export pack
          </ActionButton>
        </div>
        <div className="px-5 py-4">
          <label className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted">Target folder</label>
          <input
            value={offlinePackPath}
            onChange={(event) => setOfflinePackPath(event.target.value)}
            className="mt-2 h-9 w-full rounded-[6px] border border-white/5 px-3 text-xs text-ink placeholder:text-muted focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
            placeholder="C:\\Aletheia\\offline-pack"
          />
          {offlinePackExport ? (
            <div className="mt-3 rounded-[6px] border border-white/5 bg-paper p-3">
              <p className="font-mono text-xs text-ink">{offlinePackExport.path}</p>
              <p className="mt-2 text-xs leading-5 text-muted">
                {offlinePackExport.assetCount} assets, {Math.round(offlinePackExport.bytesWritten / 1024 / 1024)} MB written. Manifest:{" "}
                {offlinePackExport.manifestPath}
              </p>
              <p className="mt-1 text-xs leading-5 text-muted">Checksums: {offlinePackExport.checksumPath}</p>
            </div>
          ) : (
            <p className="mt-3 text-xs text-muted">No offline pack exported yet.</p>
          )}
        </div>
      </div>

      <div className="rounded-[6px] border border-white/5 bg-white/5">
        <div className="grid grid-cols-[minmax(220px,1fr)_110px_90px_120px] border-b border-white/5 px-4 py-3 text-xs font-semibold uppercase tracking-[0.12em] text-muted">
          <span>Offline asset</span>
          <span>State</span>
          <span>Size</span>
          <span>Action</span>
        </div>
        <div className="divide-y divide-line">
          {readiness.offlineAssets.assets.map((asset) => (
            <div
              key={asset.id}
              className="grid grid-cols-[minmax(220px,1fr)_110px_90px_120px] items-center px-4 py-4"
            >
              <div>
                <p className="text-sm font-semibold text-ink">{asset.label}</p>
                <p className="text-xs text-muted">{asset.language}</p>
                <div className="mt-3 grid gap-2">
                  <label className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted">Local file path</label>
                  <input
                    value={assetPaths[asset.id] ?? ""}
                    onChange={(event) => setAssetPaths((current) => ({ ...current, [asset.id]: event.target.value }))}
                    placeholder="C:\\Aletheia\\offline-assets\\stt-hausa.bin"
                    className="h-9 rounded-[6px] border border-white/5 px-3 text-xs text-ink placeholder:text-muted focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
                  />
                  <label className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted">SHA-256 checksum</label>
                  <input
                    value={assetChecksums[asset.id] ?? ""}
                    onChange={(event) => setAssetChecksums((current) => ({ ...current, [asset.id]: event.target.value }))}
                    placeholder="Paste the vendor checksum here"
                    className="h-9 rounded-[6px] border border-white/5 px-3 text-xs text-ink placeholder:text-muted focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
                  />
                </div>
              </div>
              <StatusPill tone={stateTone(asset.state)} label={asset.state} />
              <div>
                <p className="font-mono text-xs text-graphite">{asset.sizeMb} MB</p>
                {asset.checksumSha256 ? (
                  <p
                    className="mt-1 font-mono text-[10px] text-muted"
                    title={`SHA-256: ${asset.checksumSha256}`}
                  >
                    {asset.checksumSha256.slice(0, 12)}…
                  </p>
                ) : null}
              </div>
              <div className="flex justify-end">
                {asset.state === "installed" ? (
                  <StatusPill tone="healthy" label="Installed" />
                ) : (
                  <div className="flex flex-col items-end gap-2">
                    <button
                      type="button"
                      className="inline-flex items-center gap-2 rounded-[6px] border border-white/5 bg-white/5 px-3 py-2 text-xs font-semibold uppercase tracking-[0.12em] text-ink transition hover:border-accent/40 hover:text-accent"
                      onClick={() => onInstallOfflineAsset?.(asset.id)}
                    >
                      <Download className="h-4 w-4" aria-hidden="true" />
                      Install
                    </button>
                    <button
                      type="button"
                      className="inline-flex items-center gap-2 rounded-[6px] border border-white/5 bg-paper px-3 py-2 text-[11px] font-semibold uppercase tracking-[0.12em] text-ink transition hover:border-accent/40 hover:text-accent"
                      onClick={() =>
                        onInstallOfflineAssetFromPath?.(
                          asset.id,
                          assetPaths[asset.id] ?? "",
                          assetChecksums[asset.id] ?? ""
                        )
                      }
                    >
                      Verify & Install
                    </button>
                  </div>
                )}
              </div>
            </div>
          ))}
          {!readiness.offlineAssets.assets.length && (
            <div className="px-4 py-5 text-sm text-muted">Offline assets will appear after the first readiness check.</div>
          )}
        </div>
      </div>

      <div className="grid gap-5 xl:grid-cols-[340px_minmax(0,1fr)]">
        <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
          <p className="text-xs font-semibold uppercase tracking-[0.16em] text-muted">Production readiness</p>
          <div className="mt-3 flex items-end justify-between gap-4">
            <p className="text-5xl font-semibold tracking-tight text-ink">{readiness.score}%</p>
            <StatusPill tone={stateTone(readiness.state)} label={readiness.state} />
          </div>
          <div className="mt-5 space-y-3">
            <Readiness icon={KeyRound} label="Secret vault" value={readiness.secretVault.state} />
            <Readiness icon={ShieldCheck} label="Plugin signing" value={readiness.pluginPolicy.state} />
            <Readiness icon={PackageCheck} label="Offline assets" value={`${readiness.offlineAssets.installedCount}/${readiness.offlineAssets.requiredCount}`} />
            <Readiness icon={Archive} label="Hardware tests" value={`${requiredDeviceCount} required`} />
          </div>
        </div>

        <div className="rounded-[6px] border border-white/5 bg-white/5">
          <div className="grid grid-cols-[minmax(180px,0.8fr)_120px_minmax(280px,1fr)] border-b border-white/5 px-4 py-3 text-xs font-semibold uppercase tracking-[0.12em] text-muted">
            <span>Release gate</span>
            <span>State</span>
            <span>Detail</span>
          </div>
          <div className="divide-y divide-line">
            {readiness.releaseGates.map((gate) => (
              <div key={gate.label} className="grid grid-cols-[minmax(180px,0.8fr)_120px_minmax(280px,1fr)] items-center px-4 py-4">
                <p className="text-sm font-semibold text-ink">{gate.label}</p>
                <StatusPill tone={stateTone(gate.state)} label={gate.state} />
                <p className="text-sm leading-6 text-muted">{gate.detail}</p>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="rounded-[6px] border border-white/5 bg-white/5">
        <div className="flex flex-col gap-4 border-b border-white/5 px-5 py-4 md:flex-row md:items-center md:justify-between">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.16em] text-muted">Local rehearsal runner</p>
            <p className="mt-2 text-sm leading-6 text-muted">
              Exercises database lookup, AI detection, preview rendering, vMix config, live safety, redaction, and proof export without internet.
            </p>
          </div>
          {rehearsalReport ? (
            <StatusPill tone={stateTone(rehearsalReport.state)} label={`${rehearsalReport.passed}/${rehearsalReport.total} passed`} />
          ) : (
            <StatusPill tone="neutral" label="not run" />
          )}
        </div>
        {rehearsalReport ? (
          <div>
            <div className="grid grid-cols-[minmax(180px,0.8fr)_120px_minmax(280px,1fr)_90px] border-b border-white/5 px-4 py-3 text-xs font-semibold uppercase tracking-[0.12em] text-muted">
              <span>Check</span>
              <span>State</span>
              <span>Detail</span>
              <span>Time</span>
            </div>
            <div className="divide-y divide-line">
              {rehearsalReport.steps.map((step) => (
                <div key={step.label} className="grid grid-cols-[minmax(180px,0.8fr)_120px_minmax(280px,1fr)_90px] items-center px-4 py-4">
                  <p className="text-sm font-semibold text-ink">{step.label}</p>
                  <StatusPill tone={stateTone(step.state)} label={step.state} />
                  <p className="text-sm leading-6 text-muted">{step.detail}</p>
                  <p className="font-mono text-xs text-graphite">{step.durationMs}ms</p>
                </div>
              ))}
            </div>
            <p className="border-t border-white/5 bg-paper px-4 py-3 font-mono text-xs leading-5 text-graphite">
              {rehearsalReport.proofPath}
            </p>
          </div>
        ) : (
          <div className="px-5 py-5">
            <p className="text-sm leading-6 text-muted">Run this after changing integrations, app data location, or offline assets.</p>
          </div>
        )}
      </div>

      <div className="grid gap-5 xl:grid-cols-2">
        <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="text-xs font-semibold uppercase tracking-[0.16em] text-muted">Security blockers</p>
              <p className="mt-2 text-sm leading-6 text-muted">{readiness.pluginPolicy.detail}</p>
            </div>
            <StatusPill tone={stateTone(readiness.pluginPolicy.state)} label={readiness.pluginPolicy.state} />
          </div>
          <div className="mt-4 space-y-2">
            {readiness.blockers.map((blocker) => (
              <p key={blocker} className="rounded-[6px] border border-white/5 bg-paper px-3 py-2 text-sm text-graphite">
                {blocker}
              </p>
            ))}
          </div>
        </div>

        <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="text-xs font-semibold uppercase tracking-[0.16em] text-muted">Support bundle</p>
              <p className="mt-2 text-sm leading-6 text-muted">{readiness.supportBundle.detail}</p>
            </div>
            <ActionButton tone="secondary" onClick={onExportSupportBundle}>
              Export
            </ActionButton>
          </div>
          {supportBundleExport ? (
            <p className="mt-4 rounded-[6px] border border-white/5 bg-paper px-3 py-2 font-mono text-xs leading-5 text-graphite">
              {supportBundleExport.path}
            </p>
          ) : (
            <p className="mt-4 text-sm leading-6 text-muted">No support bundle exported in this session.</p>
          )}
        </div>
      </div>

      <div className="rounded-[6px] border border-white/5 bg-white/5">
        <div className="grid grid-cols-[minmax(180px,0.8fr)_130px_minmax(260px,1fr)_120px] border-b border-white/5 px-4 py-3 text-xs font-semibold uppercase tracking-[0.12em] text-muted">
          <span>Device</span>
          <span>State</span>
          <span>Next proof</span>
          <span>Required</span>
        </div>
        <div className="divide-y divide-line">
          {readiness.acceptanceDevices.map((device) => (
            <div key={device.id} className="grid grid-cols-[minmax(180px,0.8fr)_130px_minmax(260px,1fr)_120px] items-center px-4 py-4">
              <p className="text-sm font-semibold text-ink">{device.name}</p>
              <StatusPill tone={stateTone(device.state)} label={device.state} />
              <p className="text-sm leading-6 text-muted">{device.steps[0]?.expected ?? "Run acceptance rehearsal."}</p>
              <p className="text-sm font-semibold text-graphite">{device.requiredForRelease ? "Yes" : "Optional"}</p>
            </div>
          ))}
        </div>
      </div>

      <div className="rounded-[6px] border border-white/5 bg-mist p-5">
        <div className="flex items-start gap-3">
          <Download className="mt-1 h-5 w-5 text-accent" aria-hidden="true" />
          <div>
            <p className="text-sm font-semibold text-ink">Low-bandwidth policy</p>
            <p className="mt-2 text-sm leading-6 text-muted">
              Model downloads pause during services. Sync sends metadata in small batches and keeps transcript text local unless an administrator exports a support bundle.
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}

function stateTone(state: string): Tone {
  if (state === "healthy" || state === "ready" || state === "installed") return "healthy";
  if (state === "blocked" || state === "offline") return "offline";
  if (state === "not-run") return "neutral";
  return "degraded";
}

function Readiness({ icon: Icon, label, value }: { icon: LucideIcon; label: string; value: string }) {
  return (
    <div className="flex items-center justify-between border-b border-white/5 pb-3 last:border-0 last:pb-0">
      <span className="flex items-center gap-2 text-sm text-graphite">
        <Icon className="h-4 w-4 text-muted" aria-hidden="true" />
        {label}
      </span>
      <span className="text-sm font-semibold text-ink">{value}</span>
    </div>
  );
}
