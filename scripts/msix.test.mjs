// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

import { describe, expect, it } from "vitest";

import { escapeXml, renderManifest, semverToMsixVersion } from "./msix.mjs";

describe("MSIX version mapping", () => {
  it.each([
    ["0.5.0", "1.5.0.0"],
    ["1.2.3", "2.2.3.0"],
    ["4.8.15-beta.2+build.7", "5.8.15.0"],
  ])("maps %s to %s", (version, expected) => {
    expect(semverToMsixVersion(version)).toBe(expected);
  });

  it.each(["1.2", "v1.2.3", "1.2.3-01", "1.65536.0", "65535.0.0"])(
    "rejects an unrepresentable version %s",
    (version) => {
      expect(() => semverToMsixVersion(version)).toThrow();
    },
  );
});

describe("MSIX manifest rendering", () => {
  const template = [
    "__IDENTITY_NAME__",
    "__PUBLISHER__",
    "__PUBLISHER_DISPLAY_NAME__",
    "__MSIX_VERSION__",
  ].join("|");

  it("escapes Partner Center identity values", () => {
    expect(escapeXml(`A&B <C> "D" 'E'`)).toBe(
      "A&amp;B &lt;C&gt; &quot;D&quot; &apos;E&apos;",
    );
  });

  it("renders every required token", () => {
    const manifest = renderManifest(
      template,
      {
        name: "GameBuddy.GGanbuAppBridge",
        publisher: "CN=Game Buddy & Company",
        publisherDisplayName: "Game Buddy",
      },
      "1.5.0.0",
    );

    expect(manifest).toBe(
      "GameBuddy.GGanbuAppBridge|CN=Game Buddy &amp; Company|Game Buddy|1.5.0.0",
    );
    expect(manifest).not.toMatch(/__[A-Z0-9_]+__/);
  });

  it("fails if the template omits a required token", () => {
    expect(() =>
      renderManifest(
        "__IDENTITY_NAME__",
        {
          name: "name",
          publisher: "publisher",
          publisherDisplayName: "display",
        },
        "1.0.0.0",
      ),
    ).toThrow("__PUBLISHER__");
  });
});
