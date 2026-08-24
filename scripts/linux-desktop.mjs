// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

import {
  chmodSync,
  mkdirSync,
  renameSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { spawnSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { homedir } from "node:os";
import { join, resolve } from "node:path";

const desktopEntryName = "com.gamebuddy.gganbu-app-bridge.desktop";
const desktopEntryReservedCharacters = /[\s"'\\><~|&;$*?#()`]/;

export function encodeDesktopString(value) {
  return value
    .replaceAll("\\", "\\\\")
    .replaceAll("\n", "\\n")
    .replaceAll("\t", "\\t")
    .replaceAll("\r", "\\r");
}

export function encodeDesktopExecArg(value) {
  const needsQuoting =
    desktopEntryReservedCharacters.test(value) || value.includes("%");
  const escaped = value.replace(/[\\"`$]/g, "\\$&").replaceAll("%", "%%");

  const serialized = encodeDesktopString(escaped);
  return needsQuoting ? `"${serialized}"` : serialized;
}

export function buildLinuxDesktopEntry({ binaryPath, iconPath }) {
  return `[Desktop Entry]
Categories=Network;
Comment=Secure local companion bridge for GGanbu.app
Exec=${encodeDesktopExecArg(binaryPath)}
Icon=${encodeDesktopString(iconPath)}
Name=GGanbu App Bridge
StartupWMClass=gganbu-app-bridge
StartupNotify=true
Terminal=false
Type=Application
`;
}

function warn(message, error) {
  const detail = error instanceof Error ? `: ${error.message}` : "";
  console.warn(`Warning: ${message}${detail}`);
}

export function installLinuxDevDesktopEntry({
  root,
  environment = process.env,
  homeDirectory = homedir(),
}) {
  const dataHome =
    environment.XDG_DATA_HOME || join(homeDirectory, ".local", "share");
  const applicationsDir = join(dataHome, "applications");
  const desktopEntryPath = join(applicationsDir, desktopEntryName);
  const developmentBinary = resolve(
    root,
    "src-tauri",
    "target",
    "debug",
    "gganbu-app-bridge",
  );
  const developmentIcon = resolve(root, "src-tauri", "icons", "icon.png");
  const temporaryPath = `${desktopEntryPath}.${randomUUID()}.tmp`;

  try {
    mkdirSync(applicationsDir, { recursive: true });

    let mode = 0o644;
    try {
      mode = statSync(desktopEntryPath).mode & 0o777;
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }

    writeFileSync(
      temporaryPath,
      buildLinuxDesktopEntry({
        binaryPath: developmentBinary,
        iconPath: developmentIcon,
      }),
      { mode, flag: "wx" },
    );
    chmodSync(temporaryPath, mode);
    renameSync(temporaryPath, desktopEntryPath);
  } catch (error) {
    try {
      unlinkSync(temporaryPath);
    } catch (cleanupError) {
      if (cleanupError?.code !== "ENOENT")
        warn(
          "Could not clean up the temporary Linux desktop entry",
          cleanupError,
        );
    }
    warn(
      `Could not install the Linux development desktop entry at ${desktopEntryPath}; continuing without desktop integration`,
      error,
    );
    return;
  }

  // GNOME may cache desktop entries; refresh it when the optional utility exists.
  const refresh = spawnDesktopDatabaseRefresh(applicationsDir);
  if (refresh.error?.code === "ENOENT") return;
  if (refresh.error || refresh.status !== 0 || refresh.signal) {
    warn(
      `Could not refresh the Linux desktop-entry cache for ${applicationsDir}`,
      refresh.error ?? new Error(`exited with status ${refresh.status}`),
    );
  }
}

function spawnDesktopDatabaseRefresh(applicationsDir) {
  return spawnSync("update-desktop-database", [applicationsDir], {
    stdio: "ignore",
    timeout: 2_000,
  });
}
