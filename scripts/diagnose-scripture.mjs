#!/usr/bin/env node
// Reads the scripture_health.json that the Tauri shell writes on startup
// (via crate::scripture_search::run_scripture_self_test) and prints a
// human-readable report. Exits 1 if any fixture failed.
//
// On Windows the file lives at:
//   %APPDATA%\com.aletheia.production\scripture_health.json
// On macOS:   ~/Library/Application Support/com.aletheia.production/scripture_health.json
// On Linux:   ~/.local/share/com.aletheia.production/scripture_health.json
//
// Usage: node scripts/diagnose-scripture.mjs [--path <file>]

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const APP_ID = "com.aletheia.production";

function defaultReportPath() {
  if (process.platform === "win32") {
    const base = process.env.APPDATA || path.join(os.homedir(), "AppData", "Roaming");
    return path.join(base, APP_ID, "scripture_health.json");
  }
  if (process.platform === "darwin") {
    return path.join(os.homedir(), "Library", "Application Support", APP_ID, "scripture_health.json");
  }
  const xdg = process.env.XDG_DATA_HOME || path.join(os.homedir(), ".local", "share");
  return path.join(xdg, APP_ID, "scripture_health.json");
}

const args = process.argv.slice(2);
const idx = args.indexOf("--path");
const target = idx >= 0 ? args[idx + 1] : defaultReportPath();

if (!fs.existsSync(target)) {
  console.error(`scripture_health.json not found at:\n  ${target}\n`);
  console.error("Launch the Aletheia desktop shell at least once (the self-test runs on startup), or pass --path explicitly.");
  process.exit(2);
}

let report;
try {
  report = JSON.parse(fs.readFileSync(target, "utf8"));
} catch (err) {
  console.error(`Could not parse ${target}: ${err.message}`);
  process.exit(2);
}

const generated = new Date(report.generated_at_ms || 0).toISOString();
console.log("Aletheia scripture self-test report");
console.log("===================================");
console.log(`Path:         ${target}`);
console.log(`Generated:    ${generated}`);
console.log(`Translation:  ${report.translation_id}`);
console.log(`Passed:       ${report.passed}`);
console.log(`Failed:       ${report.failed}`);
console.log("");

const pad = (s, n) => (s + " ".repeat(n)).slice(0, n);
console.log(pad("KIND", 12) + pad("STATUS", 8) + pad("QUERY", 40) + "RESULT");
console.log("-".repeat(100));
for (const f of report.fixtures || []) {
  const status = f.passed ? "PASS" : "FAIL";
  const matched = f.matched_reference || "—";
  console.log(pad(f.kind, 12) + pad(status, 8) + pad(f.query, 40) + `${matched}  (${f.note})`);
}

process.exit(report.failed > 0 ? 1 : 0);
