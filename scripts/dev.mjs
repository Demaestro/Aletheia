/**
 * dev.mjs — Fast launcher for Aletheia.
 *
 * How it works:
 *   1. Scan src/ for changes.  If dist/ is stale (or missing), run
 *      `vite build` to produce a pre-compiled static bundle.
 *   2. Serve that bundle with `vite preview` — a tiny static HTTP server
 *      that starts in under one second.
 *   3. Launch the pre-built Tauri binary.  WebView2 loads local static
 *      files, which takes under 2 seconds — no TypeScript cold-compilation,
 *      no "(Not Responding)" freeze.
 *
 * Startup time:
 *   Dist up-to-date  →  < 3 s total
 *   Dist stale       →  ~10 s (vite build) + < 3 s = ~13 s
 *
 * Usage:  npm run desktop:launch
 */

import { spawn, execFileSync }       from "node:child_process";
import { resolve, dirname }          from "node:path";
import { fileURLToPath }             from "node:url";
import { existsSync, statSync, readdirSync } from "node:fs";

const __dir   = dirname(fileURLToPath(import.meta.url));
const root    = resolve(__dir, "..");
const BINARY  = "C:\\Users\\USER\\cargo-targets\\worship-production-interface\\debug\\aletheia-desktop.exe";
const PORT    = "5178";

// ── 1. Staleness check ────────────────────────────────────────────────────
function newestMtime(dir, exts) {
  let max = 0;
  try {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (entry.name.startsWith(".") || entry.name === "node_modules") continue;
      const full = resolve(dir, entry.name);
      if (entry.isDirectory()) {
        max = Math.max(max, newestMtime(full, exts));
      } else if (exts.some(e => entry.name.endsWith(e))) {
        max = Math.max(max, statSync(full).mtimeMs);
      }
    }
  } catch { /* ignore unreadable dirs */ }
  return max;
}

function distIsStale() {
  const distIndex = resolve(root, "dist", "index.html");
  if (!existsSync(distIndex)) return true;
  const distMtime = statSync(distIndex).mtimeMs;
  const srcMtime  = newestMtime(resolve(root, "src"), [".ts", ".tsx", ".css"]);
  const cfgMtime  = Math.max(
    safeStatMs(resolve(root, "vite.config.ts")),
    safeStatMs(resolve(root, "tailwind.config.js")),
    safeStatMs(resolve(root, "tailwind.config.ts")),
    safeStatMs(resolve(root, "index.html"))
  );
  return Math.max(srcMtime, cfgMtime) > distMtime;
}
function safeStatMs(p) { try { return statSync(p).mtimeMs; } catch { return 0; } }

// ── 2. Build if stale ─────────────────────────────────────────────────────
if (distIsStale()) {
  process.stdout.write("Frontend changed — building…\n");
  try {
    execFileSync("npx", ["vite", "build"], {
      cwd: root, shell: true, stdio: "inherit"
    });
    process.stdout.write("Build complete.\n");
  } catch {
    process.stderr.write("Build failed. Aborting.\n");
    process.exit(1);
  }
} else {
  process.stdout.write("dist/ is current — skipping build.\n");
}

// ── 3. Start vite preview (static server, < 1 s startup) ──────────────────
process.stdout.write(`Starting preview server on :${PORT}…\n`);

const preview = spawn("npx", ["vite", "preview", "--port", PORT], {
  cwd: root, stdio: ["ignore", "pipe", "pipe"], shell: true
});

let launched = false;

function mayLaunch(text) {
  if (launched) return;
  if (text.includes(`:${PORT}`) || text.includes("preview")) {
    launched = true;
    launchApp();
  }
}

preview.stdout.on("data", d => { const t = d.toString(); process.stdout.write(t); mayLaunch(t); });
preview.stderr.on("data", d => { const t = d.toString(); process.stderr.write(t); mayLaunch(t); });
preview.on("exit", code => {
  if (code !== 0 && code !== null) { process.stderr.write(`Preview exited (${code})\n`); process.exit(code); }
});

// Fallback: launch after 2 s even without confirmation
setTimeout(() => { if (!launched) { launched = true; launchApp(); } }, 2000);

// ── 4. Launch binary ──────────────────────────────────────────────────────
function launchApp() {
  process.stdout.write(`\nLaunching Aletheia…\n`);
  const app = spawn(BINARY, [], { cwd: root, stdio: "inherit" });
  app.on("exit", code => { preview.kill("SIGTERM"); process.exit(code ?? 0); });
  process.on("SIGINT", () => { app.kill("SIGTERM"); preview.kill("SIGTERM"); process.exit(0); });
}
