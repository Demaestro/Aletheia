// Patch the 11 deadlock call sites: change record_audit_state(&state, ...) inside
// these functions to record_audit(&store, ...) so we use the already-held lock.
import { readFileSync, writeFileSync } from "fs";

const path = "C:/Users/USER/OneDrive/Desktop/worship-production-interface/src-tauri/src/lib.rs";
const src = readFileSync(path, "utf8");
const lines = src.split("\n");

const deadlockFns = [
  "analyze_transcript", "update_vmix_config", "install_offline_asset",
  "install_offline_asset_from_path", "record_device_acceptance",
  "enable_plugin_manifest", "run_local_rehearsal", "export_support_bundle",
  "export_offline_asset_pack", "revoke_trusted_plugin", "record_calibration_sample",
];

let totalPatched = 0;

for (const fnName of deadlockFns) {
  let fnLine = -1;
  for (let i = 0; i < lines.length; i++) {
    if (new RegExp(`\\bfn ${fnName}\\b`).test(lines[i])) { fnLine = i; break; }
  }
  if (fnLine < 0) { console.log("NOT FOUND:", fnName); continue; }

  let fnEnd = lines.length;
  for (let i = fnLine + 1; i < lines.length; i++) {
    if (/\bfn [a-z_]+\(/.test(lines[i])) { fnEnd = i; break; }
  }

  // Within this function range, rewrite `record_audit_state(` → `record_audit(`
  // AND on the next line replace `        &state,` → `        &store,`
  let patched = 0;
  for (let i = fnLine; i < fnEnd; i++) {
    if (/record_audit_state\(/.test(lines[i])) {
      lines[i] = lines[i].replace("record_audit_state(", "record_audit(");
      // Look for &state, on this line OR the next one
      if (/&state,/.test(lines[i])) {
        lines[i] = lines[i].replace("&state,", "&store,");
      } else if (i + 1 < fnEnd && /&state,/.test(lines[i + 1])) {
        lines[i + 1] = lines[i + 1].replace("&state,", "&store,");
      }
      patched++;
    }
  }
  console.log(`${fnName}: patched ${patched} call(s)`);
  totalPatched += patched;
}

console.log(`\nTotal patched: ${totalPatched}`);
writeFileSync(path, lines.join("\n"));
console.log("written");
