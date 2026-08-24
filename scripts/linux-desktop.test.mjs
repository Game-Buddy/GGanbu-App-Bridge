// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

import { describe, expect, it } from "vitest";
import {
  buildLinuxDesktopEntry,
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
      '"/home/alice/Game\\$Buddy/src-tauri/target/debug/gganbu-app-bridge"',
    );
  });

  it("uses the encoded executable in the desktop entry", () => {
    expect(
      buildLinuxDesktopEntry({
        binaryPath: "/home/alice/Game Buddy/gganbu-app-bridge",
        iconPath: "/home/alice/Game Buddy/icon.png",
      }),
    ).toContain('Exec="/home/alice/Game Buddy/gganbu-app-bridge"');
  });
});
