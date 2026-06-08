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
    tauriDriver = spawn(
      path.resolve(os.homedir(), ".cargo", "bin", "tauri-driver"),
      [],
      { stdio: [null, process.stdout, process.stderr] },
    );
  },

  afterSession: () => {
    tauriDriver?.kill();
  },
};
