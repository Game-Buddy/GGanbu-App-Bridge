// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const build = spawnSync(
  "cargo",
  [
    "build",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--locked",
    "--example",
    "protocol-fixture",
  ],
  { cwd: root, stdio: "inherit" },
);
if (build.error) throw build.error;
if (build.status !== 0) process.exit(build.status ?? 1);
const binary = resolve(
  root,
  "src-tauri/target/debug/examples",
  process.platform === "win32" ? "protocol-fixture.exe" : "protocol-fixture",
);
const result = spawnSync(
  process.execPath,
  [
    resolve(root, "node_modules/vitest/vitest.mjs"),
    "run",
    "src/security/bridge.integration.test.ts",
  ],
  {
    cwd: root,
    stdio: "inherit",
    env: { ...process.env, GGANBU_BRIDGE_TEST_BINARY: binary },
  },
);
if (result.error) throw result.error;
process.exit(result.status ?? 1);
