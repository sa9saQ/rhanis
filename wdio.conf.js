// koe Windows E2E (koe-ef8 Step A) — drive the real Tauri debug build via
// tauri-driver + WebdriverIO on a native Windows host. Smoke only: proves the
// app BOOTS to the onboarding screen. No mic / no real API — the audio bridge
// stays idle until a session starts (src-tauri/src/lib.rs, koe-flu "device open
// happens inside start_session"), so a runner with no microphone is fine.
//
// Versions follow the Tauri 2 official WebdriverIO example (@wdio/* v9).
// THIS CANNOT RUN UNDER WSL: it targets the Windows WebView2, not the Linux
// WebKitGTK that a WSL build links. It is a CI-only (windows-latest) / native-
// Windows suite. Rationale: ~/research/wsl-vs-windows-ai-e2e-2026/report.md.
import { spawn, spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));

// Tauri productName is "koe" (src-tauri/tauri.conf.json) → koe.exe on Windows.
const application = path.resolve(
  here,
  "src-tauri",
  "target",
  "debug",
  process.platform === "win32" ? "koe.exe" : "koe",
);

/** @type {import("node:child_process").ChildProcess | undefined} */
let tauriDriver;

export const config = {
  // tauri-driver runs a WebDriver server on 127.0.0.1:4444 (its default port).
  // wdio MUST be pointed at it as a remote driver — without hostname/port it
  // errors "No browserName defined nor hostname or port found" (it would try to
  // launch a local browser). browserName is NOT needed: tauri:options.application
  // tells tauri-driver which binary to spawn. (Tauri 2 official wdio example.)
  hostname: "127.0.0.1",
  port: 4444,
  runner: "local",
  specs: ["./e2e/**/*.e2e.js"],
  maxInstances: 1,
  capabilities: [
    {
      maxInstances: 1,
      "tauri:options": { application },
    },
  ],
  reporters: ["spec"],
  framework: "mocha",
  mochaOpts: { ui: "bdd", timeout: 120000 },
  logLevel: "info",

  // Build the debug binary once before the session. The official example uses
  // the tauri CLI with --debug --no-bundle so only the .exe is produced (no
  // installer bundle needed for E2E). Fail loud if the build fails.
  onPrepare: () => {
    const r = spawnSync("pnpm", ["tauri", "build", "--debug", "--no-bundle"], {
      stdio: "inherit",
      shell: true,
    });
    if (r.status !== 0) {
      throw new Error(`tauri debug build failed (exit ${r.status})`);
    }
  },

  // tauri-driver bridges WebDriver <-> the platform WebView server
  // (msedgedriver on Windows). Installed in CI via `cargo install tauri-driver`.
  beforeSession: () => {
    // Windows: `cargo install tauri-driver` produces tauri-driver.exe; spawn
    // needs the extension or it ENOENTs. tauri-driver then listens on :4444.
    const driverBin = path.resolve(
      os.homedir(),
      ".cargo",
      "bin",
      process.platform === "win32" ? "tauri-driver.exe" : "tauri-driver",
    );
    tauriDriver = spawn(driverBin, [], {
      stdio: [null, process.stdout, process.stderr],
    });
    // spawn() returns immediately, but tauri-driver needs a moment to bind
    // 127.0.0.1:4444. Opening a wdio session before it listens is a race
    // (flaky in CI). Poll the WebDriver /status endpoint until it answers;
    // fail loud after 30s instead of letting wdio hit a connection refused.
    return new Promise((resolve, reject) => {
      const startTime = Date.now();
      const checkReady = async () => {
        try {
          const response = await fetch("http://127.0.0.1:4444/status");
          if (response.ok) {
            resolve();
          } else {
            throw new Error("not ready");
          }
        } catch {
          if (Date.now() - startTime > 30000) {
            tauriDriver?.kill();
            reject(new Error("tauri-driver did not become ready within 30s"));
          } else {
            setTimeout(checkReady, 500);
          }
        }
      };
      setTimeout(checkReady, 1000);
    });
  },

  afterSession: () => {
    tauriDriver?.kill();
  },
};
