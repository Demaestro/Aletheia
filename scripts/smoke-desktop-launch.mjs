import { execFileSync, spawn } from "node:child_process";
import { existsSync, statSync } from "node:fs";
import { join } from "node:path";
import os from "node:os";

const timeoutMs = Number(process.env.ALETHEIA_DESKTOP_SMOKE_TIMEOUT_MS || 90_000);
const configuredViteUrl = process.env.ALETHEIA_VITE_URL || "http://127.0.0.1:5178";
const candidateUrls = [...new Set([configuredViteUrl, "http://localhost:5178", "http://127.0.0.1:5178"])];
const startedAt = Date.now();

function scriptureHealthPath() {
  if (process.platform === "win32") {
    const base = process.env.APPDATA || join(os.homedir(), "AppData", "Roaming");
    return join(base, "com.aletheia.production", "scripture_health.json");
  }
  if (process.platform === "darwin") {
    return join(os.homedir(), "Library", "Application Support", "com.aletheia.production", "scripture_health.json");
  }
  const xdg = process.env.XDG_DATA_HOME || join(os.homedir(), ".local", "share");
  return join(xdg, "com.aletheia.production", "scripture_health.json");
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForFrontend(getProcessState) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    const processState = getProcessState();
    if (processState.exited) {
      throw new Error(
        `Desktop launch exited before frontend became reachable (code=${processState.code}, signal=${processState.signal}).`
      );
    }

    for (const url of candidateUrls) {
      try {
        const response = await fetch(url, { cache: "no-store" });
        const text = await response.text();
        if (response.ok && /<!doctype html>|<div id="root">/i.test(text)) {
          return url;
        }
        lastError = new Error(`${url} returned HTTP ${response.status}`);
      } catch (error) {
        lastError = error;
      }
    }

    await sleep(500);
  }

  throw new Error(`Frontend did not become reachable: ${lastError?.message ?? "timeout"}`);
}

async function waitForBackendHealth(getProcessState) {
  const deadline = Date.now() + timeoutMs;
  const target = scriptureHealthPath();
  while (Date.now() < deadline) {
    const processState = getProcessState();
    if (processState.exited) {
      throw new Error(
        `Desktop launch exited before backend scripture diagnostics refreshed (code=${processState.code}, signal=${processState.signal}).`
      );
    }
    if (existsSync(target) && statSync(target).mtimeMs >= startedAt) {
      return target;
    }
    if (
      /\[startup\] semantic warm skipped|\[startup\] scripture self-test skipped|\[capture\] first audio chunk received|\[self-test\] wrote/i.test(
        output
      )
    ) {
      return "startup-ready";
    }
    await sleep(500);
  }
  throw new Error(`Backend startup readiness did not complete; last diagnostics path was ${target}`);
}

const child = process.platform === "win32"
  ? spawn("cmd.exe", ["/d", "/s", "/c", "npm run desktop:launch:fast"], {
      cwd: process.cwd(),
      env: {
        ...process.env,
        RUST_BACKTRACE: "1",
      },
      stdio: ["ignore", "pipe", "pipe"],
    })
  : spawn("npm", ["run", "desktop:launch:fast"], {
  cwd: process.cwd(),
  env: {
    ...process.env,
    RUST_BACKTRACE: "1",
  },
  stdio: ["ignore", "pipe", "pipe"],
});

let output = "";
let exited = false;
let exitCode = null;
let exitSignal = null;

child.on("exit", (code, signal) => {
  exited = true;
  exitCode = code;
  exitSignal = signal;
});

for (const stream of [child.stdout, child.stderr]) {
  stream.on("data", (chunk) => {
    output += chunk.toString();
    if (output.length > 16_000) output = output.slice(-16_000);
  });
}

function killChildTree() {
  if (!child.pid) return;
  if (process.platform === "win32") {
    try {
      execFileSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], { stdio: "ignore" });
      return;
    } catch {
      // Fall through to normal kill for non-console test hosts.
    }
  }
  child.kill("SIGTERM");
  setTimeout(() => child.kill("SIGKILL"), 2000).unref();
}

try {
  const frontend = await waitForFrontend(() => ({ exited, code: exitCode, signal: exitSignal }));
  const scriptureHealth = await waitForBackendHealth(() => ({ exited, code: exitCode, signal: exitSignal }));
  if (/\bpanicked at\b|\[CRASH\]|error while running Aletheia/i.test(output)) {
    throw new Error(`Desktop emitted startup failure:\n${output}`);
  }
  console.log(JSON.stringify({ ok: true, frontend, scriptureHealth }, null, 2));
} catch (error) {
  const logTail = output.trim() ? `\n\nProcess output:\n${output.trim()}` : "";
  throw new Error(`${error.message}${logTail}`);
} finally {
  killChildTree();
}
