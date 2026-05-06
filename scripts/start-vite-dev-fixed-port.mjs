import { execSync, spawn } from "node:child_process";

const PORT = "5178";

if (process.platform === "win32") {
  try {
    const out = execSync(`netstat -ano | findstr :${PORT}`, { encoding: "utf8" });
    const pids = new Set();
    for (const line of out.split(/\r?\n/)) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      if (!/\bLISTENING\b/i.test(trimmed)) continue;
      const match = trimmed.match(/\s(\d+)$/);
      if (match) pids.add(match[1]);
    }
    for (const pid of pids) {
      try {
        execSync(`taskkill /F /PID ${pid}`, { stdio: "ignore" });
      } catch {
        // Another process may have already exited.
      }
    }
    if (pids.size > 0) {
      process.stdout.write(`Freed Aletheia dev port ${PORT}.\n`);
    }
  } catch {
    // findstr exits non-zero when no process owns the port.
  }
}

const child = spawn(
  process.platform === "win32" ? "node.exe" : "node",
  ["./node_modules/vite/bin/vite.js", "--host", "127.0.0.1", "--port", PORT, "--strictPort"],
  {
    cwd: process.cwd(),
    stdio: "inherit",
    shell: false,
  },
);

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 0);
});
