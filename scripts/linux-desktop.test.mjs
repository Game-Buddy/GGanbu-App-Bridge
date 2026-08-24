// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

import { describe, expect, it } from "vitest";
import {
  buildLinuxDesktopEntry,
  encodeDesktopString,
  encodeDesktopExecArg,
} from "./linux-desktop.mjs";

describe("Linux desktop entry", () => {
  it("quotes checkout paths containing spaces", () => {
    expect(
      encodeDesktopExecArg(
        "/home/alice/Game Buddy/src-tauri/target/debug/gganbu-app-bridge",
      ),
    ).toBe('"/home/alice/Game Buddy/src-tauri/target/debug/gganbu-app-bridge"');
  });

  it("escapes reserved Exec characters", () => {
    expect(
      encodeDesktopExecArg(
        "/home/alice/Game$Buddy/src-tauri/target/debug/gganbu-app-bridge",
      ),
    ).toBe(
      '"/home/alice/Game\\\\$Buddy/src-tauri/target/debug/gganbu-app-bridge"',
    );
  });

  it("serializes backslashes in Exec arguments", () => {
    const path = String.raw`/home/alice/Game\Buddy/src-tauri/target/debug/gganbu-app-bridge`;

    expect(encodeDesktopExecArg(path)).toBe(
      String.raw`"/home/alice/Game\\\\Buddy/src-tauri/target/debug/gganbu-app-bridge"`,
    );
  });

  it("serializes the executable and icon in the desktop entry", () => {
    const iconPath = String.raw`/home/alice/Game\Buddy/icon.png`;

    expect(
      buildLinuxDesktopEntry({
        binaryPath: "/home/alice/Game Buddy/gganbu-app-bridge",
        iconPath,
      }),
    ).toContain(
      'Exec="/home/alice/Game Buddy/gganbu-app-bridge"\nIcon=/home/alice/Game\\\\Buddy/icon.png',
    );
  });

  it("escapes Desktop Entry string values", () => {
    expect(encodeDesktopString(String.raw`Game\Buddy`)).toBe(
      String.raw`Game\\Buddy`,
    );
  });
});
