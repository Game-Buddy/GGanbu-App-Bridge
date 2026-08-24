// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

import { mkdirSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { homedir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const tauriCli = resolve(
  root,
  "node_modules",
  "@tauri-apps",
  "cli",
  "tauri.js",
);
const args = process.argv.slice(2);
const separatorIndex = args.indexOf("--");

// pnpm 11 preserves this separator, but every following argument belongs to Tauri.
if (separatorIndex >= 0) {
  args.splice(separatorIndex, 1);
}
const command =
  process.platform === "win32"
    ? process.execPath
    : resolve(root, "node_modules", ".bin", "tauri");
const commandArgs = process.platform === "win32" ? [tauriCli, ...args] : args;

if (process.platform === "linux" && args[0] === "dev") {
  const dataHome =
    process.env.XDG_DATA_HOME ?? join(homedir(), ".local", "share");
  const applicationsDir = join(dataHome, "applications");
  const desktopEntryPath = join(
    applicationsDir,
    "com.gamebuddy.gganbu-app-bridge.desktop",
  );
  const developmentBinary = resolve(
    root,
    "src-tauri",
    "target",
    "debug",
    "gganbu-app-bridge",
  );
  const developmentIcon = resolve(root, "src-tauri", "icons", "icon.png");

  mkdirSync(applicationsDir, { recursive: true });
  writeFileSync(
    desktopEntryPath,
    `[Desktop Entry]
Categories=Network;
Comment=Secure local companion bridge for GGanbu.app
Exec=${developmentBinary}
Icon=${developmentIcon}
Name=GGanbu App Bridge
StartupWMClass=gganbu-app-bridge
StartupNotify=true
Terminal=false
Type=Application
`,
    { mode: 0o644 },
  );

  // GNOME may cache desktop entries; refresh it when the optional utility exists.
  spawnSync("update-desktop-database", [applicationsDir], {
    stdio: "ignore",
  });
}

const result = spawnSync(command, commandArgs, {
  cwd: root,
  stdio: "inherit",
});

if (result.error) {
  throw result.error;
}

process.exit(result.status ?? 1);
