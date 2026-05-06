import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";

const command = process.argv[2];
const args = process.argv.slice(3);

if (!command) {
  console.error("Usage: node scripts/vector-kb-runner.mjs <install|build|serve|search|status> [args...]");
  process.exit(2);
}

const envPython = process.env.ALETHEIA_VECTOR_PYTHON;
const venvPython = process.platform === "win32"
  ? join(".venv-vector", "Scripts", "python.exe")
  : join(".venv-vector", "bin", "python");
const candidates = [
  ...(envPython ? [[envPython, []]] : []),
  ...(process.platform === "win32"
    ? [[venvPython, []], ["py", ["-3"]], ["python", []], ["python3", []]]
    : [["/tmp/aletheia-vector-venv/bin/python", []], [venvPython, []], ["python3", []], ["python", []]])
];

function ensureLocalVenv() {
  if (envPython || existsSync(venvPython)) {
    return;
  }

  const creators = process.platform === "win32"
    ? [["py", ["-3"]], ["python", []], ["python3", []]]
    : [["python3", []], ["python", []]];

  let last;
  for (const [bin, baseArgs] of creators) {
    const result = spawnSync(bin, [...baseArgs, "-m", "venv", ".venv-vector"], {
      stdio: "inherit",
      shell: false,
    });
    if (result.error) {
      last = result.error;
      continue;
    }
    if (result.status === 0 && existsSync(venvPython)) {
      return;
    }
    last = new Error(`${bin} exited with ${result.status ?? "unknown status"}`);
  }

  console.error(`Could not create .venv-vector. Last error: ${last?.message ?? "unknown"}`);
  process.exit(127);
}

function runPython(extraArgs) {
  let last;
  for (const [bin, baseArgs] of candidates) {
    const result = spawnSync(bin, [...baseArgs, ...extraArgs], {
      stdio: "inherit",
      shell: false,
    });
    if (result.error) {
      last = result.error;
      continue;
    }
    process.exit(result.status ?? 0);
  }
  console.error(`Python was not found. Last error: ${last?.message ?? "unknown"}`);
  process.exit(127);
}

const script = join("scripts", "vector_kb.py");
if (command !== "install" && !existsSync(script)) {
  console.error(`Missing ${script}`);
  process.exit(1);
}

if (command === "install") {
  ensureLocalVenv();
  runPython(["-m", "pip", "install", "-r", join("scripts", "vector-kb-requirements.txt"), ...args]);
}

if (["build", "serve", "search"].includes(command)) {
  runPython([script, command, ...args]);
}

if (command === "status") {
  const baseUrl = (process.env.ALETHEIA_VECTOR_KB_URL || "http://127.0.0.1:47618")
    .replace(/\/+$/, "")
    .replace(/\/search$/, "");
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 1200);
  try {
    const response = await fetch(`${baseUrl}/health`, { signal: controller.signal });
    const body = await response.text();
    console.log(body);
    process.exit(response.ok ? 0 : 1);
  } catch (error) {
    console.error(`Vector KB offline: ${error.message}`);
    process.exit(1);
  } finally {
    clearTimeout(timeout);
  }
}

console.error(`Unknown vector KB command: ${command}`);
process.exit(2);
