// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

import { spawnSync } from "node:child_process";

const args = process.argv.slice(2);
const separatorIndex = args.indexOf("--");

// pnpm 11 preserves this separator, but every following argument belongs to Tauri.
if (separatorIndex >= 0) {
  args.splice(separatorIndex, 1);
}
const result = spawnSync("tauri", args, {
  stdio: "inherit",
  shell: process.platform === "win32",
});

if (result.error) {
  throw result.error;
}

process.exit(result.status ?? 1);
