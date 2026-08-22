// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const args = process.argv.slice(2);
const separatorIndex = args.indexOf("--");

// pnpm 11 preserves this separator, but every following argument belongs to Tauri.
if (separatorIndex >= 0) {
  args.splice(separatorIndex, 1);
}
const result =
  process.platform === "win32"
    ? spawnSync(
        process.env.ComSpec ?? "cmd.exe",
        [
          "/d",
          "/s",
          "/c",
          resolve(root, "node_modules", ".bin", "tauri.cmd"),
          ...args,
        ],
        { cwd: root, stdio: "inherit" },
      )
    : spawnSync(resolve(root, "node_modules", ".bin", "tauri"), args, {
        cwd: root,
        stdio: "inherit",
      });

if (result.error) {
  throw result.error;
}

process.exit(result.status ?? 1);
