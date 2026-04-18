import { useEffect, useRef, useState } from "react";
import { Network, Download, Upload, RotateCcw, ShieldCheck, ShieldAlert, Key } from "lucide-react";
import { ActionButton, SectionHeader, StatusPill, cn } from "./Primitives";
import { useServicePlanStore } from "../store/useServicePlanStore";
import { useSongLibraryStore } from "../store/useSongLibraryStore";
import { useStreamOverlayStore } from "../store/useStreamOverlayStore";
import {
  type SignedFleetBundle,
  type FleetVerifyResult,
  getFleetPublicKey,
  signFleetBundle,
  verifyFleetBundle
} from "../services/desktopApi";

type FleetBundle = {
  version: 1;
  generatedAtMs: number;
  deviceLabel: string;
  plan: ReturnType<typeof useServicePlanStore.getState>["plan"];
  songs: ReturnType<typeof useSongLibraryStore.getState>["songs"];
  overlay: Pick<ReturnType<typeof useStreamOverlayStore.getState>, "tickerText" | "armed" | "liveReference">;
};

export function FleetSyncPanel({ deviceLabel }: { deviceLabel: string }) {
  const [notice, setNotice] = useState<string>("");
  const [importPreview, setImportPreview] = useState<FleetBundle | null>(null);
  const [signedImport, setSignedImport] = useState<SignedFleetBundle | null>(null);
  const [verifyResult, setVerifyResult] = useState<FleetVerifyResult | null>(null);
  const [localPublicKey, setLocalPublicKey] = useState<string | null>(null);
  const [working, setWorking] = useState<"sign" | "verify" | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let cancelled = false;
    void getFleetPublicKey()
      .then((pk) => {
        if (!cancelled) setLocalPublicKey(pk);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  const exportBundle = async () => {
    const bundle: FleetBundle = {
      version: 1,
      generatedAtMs: Date.now(),
      deviceLabel,
      plan: useServicePlanStore.getState().plan,
      songs: useSongLibraryStore.getState().songs,
      overlay: {
        tickerText: useStreamOverlayStore.getState().tickerText,
        armed: useStreamOverlayStore.getState().armed,
        liveReference: useStreamOverlayStore.getState().liveReference
      }
    };
    // Canonical JSON: stable key order via JSON.stringify on a plain object
    // (bundle fields are declared in a fixed order, so this is deterministic for
    // signing. Receiving side verifies byte-for-byte.)
    const payloadJson = JSON.stringify(bundle, null, 2);
    setWorking("sign");
    try {
      const signed = await signFleetBundle(payloadJson, deviceLabel);
      const envelope = {
        ...signed,
        bundle, // convenience — verifier uses payloadJson, this is for humans
      };
      const blob = new Blob([JSON.stringify(envelope, null, 2)], {
        type: "application/json;charset=utf-8"
      });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `aletheia-fleet-bundle-${new Date().toISOString().replace(/[:.]/g, "-")}.json`;
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
      setNotice(
        `Signed bundle exported (${bundle.songs.length} songs, ${bundle.plan?.items.length ?? 0} plan items, sha256=${signed.payloadSha256.slice(0, 12)}…).`
      );
    } catch (err) {
      setNotice(err instanceof Error ? `Signing failed: ${err.message}` : "Signing failed.");
    } finally {
      setWorking(null);
    }
  };

  const pickFile = () => fileRef.current?.click();

  const onFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      const parsed = JSON.parse(text) as
        | (SignedFleetBundle & { bundle?: FleetBundle })
        | FleetBundle;

      // Signed envelope? Verify through Rust, then treat the payloadJson as truth.
      if ("signatureHex" in parsed && "publicKeyHex" in parsed && "payloadJson" in parsed) {
        setWorking("verify");
        const envelope: SignedFleetBundle = {
          signatureHex: parsed.signatureHex,
          publicKeyHex: parsed.publicKeyHex,
          payloadSha256: parsed.payloadSha256,
          payloadJson: parsed.payloadJson,
          signedAtMs: parsed.signedAtMs,
          signerLabel: parsed.signerLabel
        };
        const res = await verifyFleetBundle(envelope);
        setVerifyResult(res);
        setSignedImport(envelope);
        const inner = JSON.parse(envelope.payloadJson) as FleetBundle;
        if (inner.version !== 1) throw new Error("Unsupported bundle version.");
        setImportPreview(inner);
        setNotice(
          res.valid
            ? `Signed bundle from ${inner.deviceLabel} verified — ready to apply.`
            : `Signature check failed: ${res.detail}. Review before applying.`
        );
      } else {
        // Legacy unsigned bundle — warn clearly.
        const legacy = parsed as FleetBundle;
        if (legacy.version !== 1) throw new Error("Unsupported bundle version.");
        setImportPreview(legacy);
        setSignedImport(null);
        setVerifyResult({
          valid: false,
          detail: "Unsigned bundle — no signature present.",
          publicKeyHex: "",
          payloadSha256: ""
        });
        setNotice(`Unsigned bundle from ${legacy.deviceLabel} loaded. Consider re-exporting from a signed source.`);
      }
    } catch (err) {
      setNotice(err instanceof Error ? err.message : "Invalid bundle.");
    } finally {
      setWorking(null);
      e.target.value = "";
    }
  };

  const applyImport = () => {
    if (!importPreview) return;
    if (verifyResult && !verifyResult.valid) {
      const proceed = window.confirm(
        `Signature check did not pass (${verifyResult.detail}). Apply bundle anyway?`
      );
      if (!proceed) return;
    }
    if (importPreview.plan) useServicePlanStore.getState().setPlan(importPreview.plan);
    for (const song of importPreview.songs) useSongLibraryStore.getState().upsertSong(song);
    if (importPreview.overlay.tickerText) useStreamOverlayStore.getState().setTickerText(importPreview.overlay.tickerText);
    setNotice(`Applied bundle from ${importPreview.deviceLabel}.`);
    setImportPreview(null);
    setSignedImport(null);
    setVerifyResult(null);
  };

  return (
    <section className="space-y-5">
      <SectionHeader
        eyebrow="Fleet sync"
        title="Client-only device bundle — sync plan, songs & overlay"
        detail="Export your plan, song library and overlay state as a signed-format JSON bundle. Carry it to a venue on a USB stick or share via your own channel — no cloud required, works across 1000 devices with identical state."
        action={<StatusPill tone="neutral" label="Client-only" detail="No telemetry" />}
      />

      <div className="grid gap-4 lg:grid-cols-2">
        <div className="space-y-3 rounded-[6px] border border-white/8 bg-white/[0.02] p-4">
          <div className="flex items-center gap-2 text-sm font-semibold text-ink">
            <Download className="h-4 w-4 text-violet-400/80" aria-hidden="true" /> Export bundle
          </div>
          <p className="text-xs text-muted">
            Captures the active service plan, the full song library (including CCLI usage metadata), and stream overlay
            ticker text. The export is ed25519-signed so receiving devices can verify origin before applying.
          </p>
          <ActionButton onClick={() => void exportBundle()} disabled={working === "sign"}>
            <Download className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
            {working === "sign" ? "Signing…" : "Export signed .json"}
          </ActionButton>
          {localPublicKey ? (
            <div className="mt-1 rounded-[6px] border border-white/8 bg-black/30 p-2 text-[11px]">
              <p className="flex items-center gap-1.5 font-semibold uppercase tracking-[0.12em] text-muted">
                <Key className="h-3 w-3" aria-hidden="true" />
                This device public key
              </p>
              <p className="mt-1 break-all font-mono text-[10px] text-violet-200/90">{localPublicKey}</p>
              <p className="mt-1 text-muted">
                Share this key out-of-band so recipients can trust bundles you sign.
              </p>
            </div>
          ) : null}
        </div>

        <div className="space-y-3 rounded-[6px] border border-white/8 bg-white/[0.02] p-4">
          <div className="flex items-center gap-2 text-sm font-semibold text-ink">
            <Upload className="h-4 w-4 text-violet-400/80" aria-hidden="true" /> Import bundle
          </div>
          <p className="text-xs text-muted">
            Loads and previews a bundle before applying. Apply merges songs and replaces the active plan &amp; overlay
            state.
          </p>
          <div className="flex flex-wrap gap-2">
            <ActionButton tone="secondary" onClick={pickFile}>
              <Upload className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
              Select .json
            </ActionButton>
            {importPreview ? (
              <>
                <ActionButton onClick={applyImport}>Apply bundle</ActionButton>
                <ActionButton tone="secondary" onClick={() => setImportPreview(null)}>
                  <RotateCcw className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
                  Discard
                </ActionButton>
              </>
            ) : null}
          </div>
          <input ref={fileRef} type="file" accept=".json,application/json" className="hidden" onChange={onFile} />
          {importPreview ? (
            <div className="rounded-[6px] border border-violet-400/30 bg-violet-500/10 p-3 text-xs text-white/85">
              <div className="flex items-center gap-2 text-violet-200">
                <Network className="h-3.5 w-3.5" aria-hidden="true" />
                <span className="font-semibold">{importPreview.deviceLabel}</span>
                <span className="text-muted">
                  · {new Date(importPreview.generatedAtMs).toLocaleString()}
                </span>
              </div>
              {verifyResult ? (
                <div
                  className={cn(
                    "mt-2 flex items-start gap-2 rounded-[6px] border px-2 py-1.5 text-[11px]",
                    verifyResult.valid
                      ? "border-emerald-400/30 bg-emerald-500/10 text-emerald-200"
                      : "border-rose-400/40 bg-rose-500/10 text-rose-200"
                  )}
                >
                  {verifyResult.valid ? (
                    <ShieldCheck className="mt-0.5 h-3.5 w-3.5" aria-hidden="true" />
                  ) : (
                    <ShieldAlert className="mt-0.5 h-3.5 w-3.5" aria-hidden="true" />
                  )}
                  <div className="min-w-0">
                    <p className="font-semibold">
                      {verifyResult.valid ? "Signature valid" : "Signature not valid"}
                    </p>
                    <p className="mt-0.5 opacity-90">{verifyResult.detail}</p>
                    {signedImport ? (
                      <p className="mt-1 break-all font-mono text-[10px] opacity-80">
                        signer: {signedImport.publicKeyHex.slice(0, 24)}… · sha256:{" "}
                        {verifyResult.payloadSha256.slice(0, 16)}…
                      </p>
                    ) : null}
                  </div>
                </div>
              ) : null}
              <div className="mt-2 grid grid-cols-3 gap-3 text-center">
                <Stat label="Plan items" value={importPreview.plan?.items.length ?? 0} />
                <Stat label="Songs" value={importPreview.songs.length} />
                <Stat label="Overlay armed" value={importPreview.overlay.armed ? "yes" : "no"} />
              </div>
            </div>
          ) : null}
        </div>
      </div>

      {notice ? (
        <p className={cn("rounded-[6px] border border-white/8 bg-white/[0.02] px-3 py-2 text-xs text-white/80")}>{notice}</p>
      ) : null}
    </section>
  );
}

function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <div>
      <p className="font-mono text-sm font-semibold text-white">{value}</p>
      <p className="text-[10px] uppercase tracking-widest text-muted">{label}</p>
    </div>
  );
}
