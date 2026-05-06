import { useEffect, useRef, useState } from "react";
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
import {
  cancelFullScriptureRegressionJob,
  getFullScriptureRegressionJob,
  startFullScriptureRegressionJob,
  type BackendDiagnostics,
  type ProductionReleaseGate,
  type ScriptureRegressionJob,
  type ScriptureRegressionReport,
} from "../services/desktopApi";
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
  backendDiagnostics,
  releaseGate,
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
  backendDiagnostics?: BackendDiagnostics;
  releaseGate?: ProductionReleaseGate;
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
  const [scriptureRegression, setScriptureRegression] = useState<ScriptureRegressionReport | null>(null);
  const [scriptureRegressionRunning, setScriptureRegressionRunning] = useState(false);
  const [scriptureRegressionError, setScriptureRegressionError] = useState<string | null>(null);
  const [scriptureRegressionJob, setScriptureRegressionJob] = useState<ScriptureRegressionJob | null>(null);
  const scriptureRegressionPollRef = useRef<number | null>(null);

  const clearScriptureRegressionPoll = () => {
    if (scriptureRegressionPollRef.current !== null) {
      window.clearTimeout(scriptureRegressionPollRef.current);
      scriptureRegressionPollRef.current = null;
    }
  };

  const pollScriptureRegressionJob = () => {
    clearScriptureRegressionPoll();
    void getFullScriptureRegressionJob()
      .then((job) => {
        setScriptureRegressionJob(job);
        if (!job) {
          setScriptureRegressionRunning(false);
          return;
        }
        if (job.report) setScriptureRegression(job.report);
        if (job.error) setScriptureRegressionError(job.error);
        if (["running", "cancelling"].includes(job.state)) {
          setScriptureRegressionRunning(true);
          scriptureRegressionPollRef.current = window.setTimeout(pollScriptureRegressionJob, 800);
        } else {
          setScriptureRegressionRunning(false);
        }
      })
      .catch((error: unknown) => {
        setScriptureRegressionRunning(false);
        setScriptureRegressionError(
          error instanceof Error ? error.message : "Could not poll scripture regression job."
        );
      });
  };

  useEffect(() => () => clearScriptureRegressionPoll(), []);

  const runCanonRegression = () => {
    if (scriptureRegressionRunning) {
      void cancelFullScriptureRegressionJob()
        .then(() => pollScriptureRegressionJob())
        .catch((error: unknown) => {
          setScriptureRegressionError(
            error instanceof Error ? error.message : "Could not cancel scripture regression."
          );
        });
      return;
    }
    setScriptureRegressionRunning(true);
    setScriptureRegressionError(null);
    void startFullScriptureRegressionJob("kjv")
      .then((job) => {
        setScriptureRegressionJob(job);
        pollScriptureRegressionJob();
      })
      .catch((error: unknown) => {
        setScriptureRegressionRunning(false);
        setScriptureRegressionError(
          error instanceof Error ? error.message : "Could not start full scripture regression."
        );
      });
  };

  return (
    <section className="space-y-5">
      <SectionHeader
        eyebrow="Offline and health"
        title="Know what still works before the service starts"
        detail="The health panel prioritizes blocking issues first and translates technical failures into operator actions."
        action={
          <div className="flex flex-wrap gap-2">
            <ActionButton tone="secondary" onClick={onRunLocalRehearsal}>
              Run local rehearsal
            </ActionButton>
            <ActionButton tone="secondary" onClick={runCanonRegression}>
              {scriptureRegressionRunning ? "Cancel canon regression" : "Run full scripture regression"}
            </ActionButton>
            <ActionButton onClick={onRunCheck}>Run pre-service check</ActionButton>
          </div>
        }
      />

      {/* ── Offline readiness score + system table ── */}
      <div className="grid gap-4 xl:grid-cols-[280px_1fr]">
        {/* Score card */}
        <div className="overflow-hidden rounded-[8px] border border-line bg-paper p-5">
          <p className="text-[11px] font-semibold uppercase tracking-widest text-muted">Offline readiness</p>
          <p className="mt-3 text-4xl font-semibold tracking-tight text-ink">{readiness.score}%</p>
          <p className="mt-3 text-sm leading-6 text-muted">
            {readiness.score >= 90
              ? "System is ready for live service. All critical offline assets are installed."
              : readiness.score >= 70
              ? "Ready for English text detection. Some offline STT packs need installation."
              : "Pre-service check required. Run readiness check to resolve blocking issues."}
          </p>
          <div className="mt-5 space-y-3">
            <Readiness icon={Database} label="Library" value="Ready" />
            <Readiness icon={Activity} label="Detection" value="Local active" />
            <Readiness icon={WifiOff} label="Internet" value="Not required" />
            <Readiness icon={HardDrive} label="Storage" value="18.4 GB" />
          </div>
        </div>

        {/* System health card list */}
        <div className="overflow-hidden rounded-[8px] border border-line bg-white/[0.03]">
          <div className="grid grid-cols-[1fr_auto] border-b border-line px-5 py-3 text-[11px] font-semibold uppercase tracking-widest text-muted">
            <span>System · Detail</span>
            <span>State</span>
          </div>
          <div className="max-h-[360px] divide-y divide-line overflow-y-auto">
            {items.map((item) => (
              <div key={item.label} className="flex items-start gap-4 px-5 py-4">
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-semibold text-ink">{item.label}</p>
                  <p className="mt-0.5 text-xs text-muted">{item.detail}</p>
                </div>
                <div className="flex shrink-0 flex-col items-end gap-2">
                  <StatusPill tone={item.state} label={item.state} />
                  <button
                    type="button"
                    className="rounded-[6px] border border-line bg-mist px-3 py-1.5 text-xs font-semibold text-ink transition hover:border-accent focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
                  >
                    {item.action}
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>

      {(backendDiagnostics || releaseGate) ? (
        <div className="grid gap-4 xl:grid-cols-2">
          {backendDiagnostics ? (
            <div className="rounded-[8px] border border-line bg-white/[0.03] p-5">
              <div className="flex items-start justify-between gap-4">
                <div>
                  <p className="text-[11px] font-semibold uppercase tracking-widest text-muted">Backend diagnostics</p>
                  <h3 className="mt-1 text-lg font-semibold tracking-tight text-ink">{backendDiagnostics.state}</h3>
                </div>
                <StatusPill tone={stateTone(backendDiagnostics.state)} label={backendDiagnostics.state} />
              </div>
              <div className="mt-4 grid gap-3 sm:grid-cols-2">
                <DiagnosticMetric label="Capture" value={backendDiagnostics.capture.running ? "running" : "stopped"} />
                <DiagnosticMetric label="STT" value={backendDiagnostics.stt.modelLoaded ? "loaded" : backendDiagnostics.stt.modelPath ? "installed" : "missing"} />
                <DiagnosticMetric label="Vector" value={backendDiagnostics.vector.state} />
                <DiagnosticMetric label="Displays" value={`${backendDiagnostics.displays.length}`} />
              </div>
              {backendDiagnostics.issues.length > 0 ? (
                <div className="mt-4 space-y-2">
                  {backendDiagnostics.issues.slice(0, 6).map((issue) => (
                    <p key={issue} className="rounded-[6px] border border-line bg-paper px-3 py-2 text-xs text-muted">{issue}</p>
                  ))}
                </div>
              ) : (
                <p className="mt-4 text-sm text-muted">No backend blockers reported.</p>
              )}
            </div>
          ) : null}

          {releaseGate ? (
            <div className="rounded-[8px] border border-line bg-white/[0.03] p-5">
              <div className="flex items-start justify-between gap-4">
                <div>
                  <p className="text-[11px] font-semibold uppercase tracking-widest text-muted">Production release gate</p>
                  <h3 className="mt-1 text-lg font-semibold tracking-tight text-ink">{releaseGate.passed}/{releaseGate.total} checks passed</h3>
                </div>
                <StatusPill tone={stateTone(releaseGate.state)} label={releaseGate.state} />
              </div>
              <div className="mt-4 max-h-72 divide-y divide-line overflow-y-auto rounded-[8px] border border-line">
                {releaseGate.checks.map((check) => (
                  <div key={check.id} className="grid gap-3 px-3 py-3 sm:grid-cols-[1fr_auto]">
                    <div className="min-w-0">
                      <p className="text-sm font-semibold text-ink">{check.label}</p>
                      <p className="mt-0.5 text-xs text-muted">{check.detail}</p>
                    </div>
                    <StatusPill tone={check.state === "pass" ? "healthy" : check.blocking ? "offline" : "degraded"} label={check.state} />
                  </div>
                ))}
              </div>
            </div>
          ) : null}
        </div>
      ) : null}

      {(scriptureRegression || scriptureRegressionError) ? (
        <div className="rounded-[8px] border border-line bg-white/[0.03] p-5">
          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="text-[11px] font-semibold uppercase tracking-widest text-muted">Full scripture regression</p>
              <h3 className="mt-1 text-lg font-semibold tracking-tight text-ink">
                {scriptureRegression
                  ? `${scriptureRegression.passed}/${scriptureRegression.versesChecked} verse references passed`
                  : scriptureRegressionJob
                  ? `Regression ${scriptureRegressionJob.state}`
                  : "Regression failed"}
              </h3>
            </div>
            {scriptureRegression ? (
              <StatusPill tone={scriptureRegression.state === "pass" ? "healthy" : "offline"} label={scriptureRegression.state} />
            ) : null}
          </div>
          {scriptureRegression ? (
            <div className="mt-4 grid gap-3 sm:grid-cols-4">
              <DiagnosticMetric label="Books" value={`${scriptureRegression.booksChecked}`} />
              <DiagnosticMetric label="Chapters" value={`${scriptureRegression.chaptersChecked}`} />
              <DiagnosticMetric label="Verses" value={`${scriptureRegression.versesChecked}`} />
              <DiagnosticMetric label="Duration" value={`${scriptureRegression.durationMs} ms`} />
              <DiagnosticMetric label="Search path" value={`${scriptureRegression.searchPathChecked}`} />
              <DiagnosticMetric label="Voice commands" value={`${scriptureRegression.voiceCommandChecked}`} />
              <DiagnosticMetric label="Partial quotes" value={`${scriptureRegression.partialQuoteChecked}`} />
            </div>
          ) : null}
          {scriptureRegressionError ? (
            <p className="mt-4 rounded-[6px] border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-sm text-amber-200">
              {scriptureRegressionError}
            </p>
          ) : null}
          {scriptureRegression?.firstFailures.length ? (
            <div className="mt-4 max-h-72 divide-y divide-line overflow-y-auto rounded-[8px] border border-line">
              {scriptureRegression.firstFailures.slice(0, 20).map((failure) => (
                <div key={`${failure.reference}-${failure.detail}`} className="px-3 py-3">
                  <p className="text-sm font-semibold text-ink">{failure.reference}</p>
                  <p className="mt-0.5 text-xs text-muted">{failure.detail}</p>
                </div>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}

      {/* ── Acceptance devices ── */}
      <details className="overflow-hidden rounded-[8px] border border-line bg-white/[0.03]">
        <summary className="cursor-pointer border-b border-line px-5 py-3 text-[11px] font-semibold uppercase tracking-widest text-muted transition hover:text-ink">
          Device acceptance testing · {requiredDeviceCount} required
        </summary>
        <div className="divide-y divide-line">
          {readiness.acceptanceDevices.map((device) => (
            <div key={device.id} className="px-5 py-4">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="min-w-0">
                  <p className="text-sm font-semibold text-ink">{device.name}</p>
                  <p className="text-xs text-muted">{device.category}</p>
                </div>
                <div className="flex shrink-0 items-center gap-3">
                  <StatusPill tone={stateTone(device.state)} label={device.state} />
                  <span className="text-xs text-muted">{device.requiredForRelease ? "Required" : "Optional"}</span>
                </div>
              </div>
              <div className="mt-3 space-y-3">
                {device.steps.map((step) => {
                  const key = `${device.id}:${step.label}`;
                  return (
                    <div key={step.label} className="space-y-2 rounded-[6px] border border-line bg-paper p-3">
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
                          className="h-8 rounded-[6px] border border-line bg-mist px-3 text-xs text-ink placeholder:text-muted focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
                        />
                        <input
                          value={deviceEvidence[key] ?? ""}
                          onChange={(event) => setDeviceEvidence((current) => ({ ...current, [key]: event.target.value }))}
                          placeholder="Evidence path (optional)"
                          className="h-8 rounded-[6px] border border-line bg-mist px-3 text-xs text-ink placeholder:text-muted focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
                        />
                      </div>
                      <div className="flex flex-wrap gap-2">
                        <button
                          type="button"
                          className="rounded-[6px] border border-emerald-500/30 bg-emerald-500/10 px-3 py-1.5 text-[11px] font-semibold uppercase tracking-[0.12em] text-emerald-600 dark:text-emerald-400 transition hover:bg-emerald-500/20"
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
                          className="rounded-[6px] border border-red-500/30 bg-red-500/10 px-3 py-1.5 text-[11px] font-semibold uppercase tracking-[0.12em] text-red-600 dark:text-red-400 transition hover:bg-red-500/20"
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
            </div>
          ))}
          {!readiness.acceptanceDevices.length && (
            <p className="px-5 py-5 text-sm text-muted">No acceptance devices registered. Run a pre-service check to populate.</p>
          )}
        </div>
      </details>

      {/* ── Offline distribution pack ── */}
      <details className="overflow-hidden rounded-[8px] border border-line bg-white/[0.03]">
        <summary className="cursor-pointer border-b border-line px-5 py-3 text-[11px] font-semibold uppercase tracking-widest text-muted transition hover:text-ink">
          Offline distribution pack
        </summary>
        <div className="flex flex-wrap items-center justify-between gap-4 border-b border-line px-5 py-4">
          <div className="min-w-0">
            <p className="text-[11px] font-semibold uppercase tracking-widest text-muted">Offline distribution pack</p>
            <p className="mt-1 text-sm text-muted">
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
            className="mt-2 h-9 w-full rounded-[6px] border border-line bg-mist px-3 text-xs text-ink placeholder:text-muted focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
            placeholder="C:\\Aletheia\\offline-pack"
          />
          {offlinePackExport ? (
            <div className="mt-3 rounded-[6px] border border-line bg-paper p-3">
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
      </details>

      {/* ── Offline assets ── */}
      <div className="overflow-hidden rounded-[8px] border border-line bg-white/[0.03]">
        <div className="grid grid-cols-[1fr_auto] border-b border-line px-5 py-3 text-[11px] font-semibold uppercase tracking-widest text-muted">
          <span>Offline asset · Language</span>
          <span>State / Action</span>
        </div>
        <div className="max-h-[520px] divide-y divide-line overflow-y-auto">
          {readiness.offlineAssets.assets.map((asset) => (
            <div key={asset.id} className="flex items-start gap-4 px-5 py-4">
              <div className="min-w-0 flex-1">
                <p className="text-sm font-semibold text-ink">{asset.label}</p>
                <p className="text-xs text-muted">{asset.language}</p>
                <div className="mt-3 grid gap-2">
                  <label className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted">Local file path</label>
                  <input
                    value={assetPaths[asset.id] ?? ""}
                    onChange={(event) => setAssetPaths((current) => ({ ...current, [asset.id]: event.target.value }))}
                    placeholder="C:\\Aletheia\\offline-assets\\stt-hausa.bin"
                    className="h-9 rounded-[6px] border border-line bg-mist px-3 text-xs text-ink placeholder:text-muted focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
                  />
                  <label className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted">SHA-256 checksum</label>
                  <input
                    value={assetChecksums[asset.id] ?? ""}
                    onChange={(event) => setAssetChecksums((current) => ({ ...current, [asset.id]: event.target.value }))}
                    placeholder="Paste the vendor checksum here"
                    className="h-9 rounded-[6px] border border-line bg-mist px-3 text-xs text-ink placeholder:text-muted focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
                  />
                </div>
              </div>
              <div className="flex shrink-0 flex-col items-end gap-2">
                <div>
                  <StatusPill tone={stateTone(asset.state)} label={asset.state} />
                  <p className="mt-1 font-mono text-xs text-graphite">{asset.sizeMb} MB</p>
                  {asset.checksumSha256 && (
                    <p className="mt-1 font-mono text-[10px] text-muted" title={`SHA-256: ${asset.checksumSha256}`}>
                      {asset.checksumSha256.slice(0, 12)}…
                    </p>
                  )}
                </div>
                {asset.state === "installed" ? (
                  <StatusPill tone="healthy" label="Installed" />
                ) : (
                  <div className="flex flex-col items-end gap-2">
                    <button
                      type="button"
                      className="inline-flex items-center gap-2 rounded-[6px] border border-line bg-mist px-3 py-2 text-xs font-semibold uppercase tracking-[0.12em] text-ink transition hover:border-accent/40 hover:text-accent"
                      onClick={() => onInstallOfflineAsset?.(asset.id)}
                    >
                      <Download className="h-4 w-4" aria-hidden="true" />
                      Install
                    </button>
                    <button
                      type="button"
                      className="inline-flex items-center gap-2 rounded-[6px] border border-line bg-paper px-3 py-2 text-[11px] font-semibold uppercase tracking-[0.12em] text-ink transition hover:border-accent/40 hover:text-accent"
                      onClick={() =>
                        onInstallOfflineAssetFromPath?.(
                          asset.id,
                          assetPaths[asset.id] ?? "",
                          assetChecksums[asset.id] ?? ""
                        )
                      }
                    >
                      Verify &amp; Install
                    </button>
                  </div>
                )}
              </div>
            </div>
          ))}
          {!readiness.offlineAssets.assets.length && (
            <div className="px-5 py-5 text-sm text-muted">Offline assets will appear after the first readiness check.</div>
          )}
        </div>
      </div>

      {/* ── Production readiness + release gates ── */}
      <div className="grid gap-5 xl:grid-cols-[280px_1fr]">
        <div className="overflow-hidden rounded-[8px] border border-line bg-paper p-5">
          <p className="text-[11px] font-semibold uppercase tracking-widest text-muted">Production readiness</p>
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

        <div className="overflow-hidden rounded-[8px] border border-line bg-white/[0.03]">
          <div className="border-b border-line px-5 py-3 text-[11px] font-semibold uppercase tracking-widest text-muted">
            Release gates
          </div>
          <div className="divide-y divide-line">
            {readiness.releaseGates.map((gate) => (
              <div key={gate.label} className="flex items-start gap-4 px-5 py-4">
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-semibold text-ink">{gate.label}</p>
                  <p className="mt-0.5 text-sm text-muted">{gate.detail}</p>
                </div>
                <span className="flex-none"><StatusPill tone={stateTone(gate.state)} label={gate.state} /></span>
              </div>
            ))}
            {!readiness.releaseGates.length && (
              <p className="px-5 py-5 text-sm text-muted">No release gates configured.</p>
            )}
          </div>
        </div>
      </div>

      {/* ── Local rehearsal runner ── */}
      <details className="overflow-hidden rounded-[8px] border border-line bg-white/[0.03]">
        <summary className="cursor-pointer border-b border-line px-5 py-3 text-[11px] font-semibold uppercase tracking-widest text-muted transition hover:text-ink">
          Local rehearsal runner
        </summary>
        <div className="flex flex-wrap items-center justify-between gap-4 border-b border-line px-5 py-4">
          <div className="min-w-0">
            <p className="text-[11px] font-semibold uppercase tracking-widest text-muted">Local rehearsal runner</p>
            <p className="mt-1 text-sm text-muted">
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
            <div className="divide-y divide-line">
              {rehearsalReport.steps.map((step) => (
                <div key={step.label} className="flex items-start gap-4 px-5 py-4">
                  <div className="min-w-0 flex-1">
                    <p className="text-sm font-semibold text-ink">{step.label}</p>
                    <p className="mt-0.5 text-xs text-muted">{step.detail}</p>
                  </div>
                  <div className="flex shrink-0 flex-col items-end gap-1">
                    <StatusPill tone={stateTone(step.state)} label={step.state} />
                    <p className="font-mono text-xs text-graphite">{step.durationMs}ms</p>
                  </div>
                </div>
              ))}
            </div>
            <p className="border-t border-line bg-paper px-5 py-3 font-mono text-xs leading-5 text-graphite">
              {rehearsalReport.proofPath}
            </p>
          </div>
        ) : (
          <div className="px-5 py-5">
            <p className="text-sm leading-6 text-muted">Run this after changing integrations, app data location, or offline assets.</p>
          </div>
        )}
      </details>

      {/* ── Security + support bundle ── */}
      <div className="grid gap-5 xl:grid-cols-2">
        <div className="overflow-hidden rounded-[8px] border border-line bg-paper p-5">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0">
              <p className="text-[11px] font-semibold uppercase tracking-widest text-muted">Security blockers</p>
              <p className="mt-2 text-sm leading-6 text-muted">{readiness.pluginPolicy.detail}</p>
            </div>
            <span className="flex-none"><StatusPill tone={stateTone(readiness.pluginPolicy.state)} label={readiness.pluginPolicy.state} /></span>
          </div>
          <div className="mt-4 space-y-2">
            {readiness.blockers.map((blocker) => (
              <p key={blocker} className="rounded-[6px] border border-line bg-mist px-3 py-2 text-sm text-graphite">
                {blocker}
              </p>
            ))}
          </div>
        </div>

        <div className="overflow-hidden rounded-[8px] border border-line bg-paper p-5">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0">
              <p className="text-[11px] font-semibold uppercase tracking-widest text-muted">Support bundle</p>
              <p className="mt-2 text-sm leading-6 text-muted">{readiness.supportBundle.detail}</p>
            </div>
            <ActionButton tone="secondary" onClick={onExportSupportBundle}>
              Export
            </ActionButton>
          </div>
          {supportBundleExport ? (
            <p className="mt-4 rounded-[6px] border border-line bg-mist px-3 py-2 font-mono text-xs leading-5 text-graphite">
              {supportBundleExport.path}
            </p>
          ) : (
            <p className="mt-4 text-sm leading-6 text-muted">No support bundle exported in this session.</p>
          )}
        </div>
      </div>

      {/* ── Low bandwidth policy notice ── */}
      <div className="flex items-start gap-3 rounded-[8px] border border-line bg-mist p-5">
        <Download className="mt-1 h-5 w-5 shrink-0 text-accent" aria-hidden="true" />
        <div>
          <p className="text-sm font-semibold text-ink">Low-bandwidth policy</p>
          <p className="mt-2 text-sm leading-6 text-muted">
            Model downloads pause during services. Sync sends metadata in small batches and keeps transcript text local unless an administrator exports a support bundle.
          </p>
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
    <div className="flex items-center justify-between border-b border-line pb-3 last:border-0 last:pb-0">
      <span className="flex items-center gap-2 text-sm text-graphite">
        <Icon className="h-4 w-4 text-muted" aria-hidden="true" />
        {label}
      </span>
      <span className="text-sm font-semibold text-ink">{value}</span>
    </div>
  );
}

function DiagnosticMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-[6px] border border-line bg-paper px-3 py-2">
      <p className="text-[10px] font-semibold uppercase tracking-widest text-muted">{label}</p>
      <p className="mt-1 truncate text-sm font-semibold text-ink">{value}</p>
    </div>
  );
}
