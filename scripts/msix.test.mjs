// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { JSDOM } from "jsdom";
import { describe, expect, it } from "vitest";

import { escapeXml, renderManifest, semverToMsixVersion } from "./msix.mjs";

describe("MSIX version mapping", () => {
  it.each([
    ["0.5.0", "1.5.0.0"],
    ["1.2.3", "2.2.3.0"],
  ])("maps %s to %s", (version, expected) => {
    expect(semverToMsixVersion(version)).toBe(expected);
  });

  it.each([
    "1.2",
    "v1.2.3",
    "1.2.3-01",
    "1.2.3-beta.1+build.7",
    "1.65536.0",
    "65535.0.0",
  ])("rejects an unrepresentable version %s", (version) => {
    expect(() => semverToMsixVersion(version)).toThrow();
  });
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

  it("declares private LAN access for the local bridge server", () => {
    const manifestXml = readFileSync(
      resolve(
        import.meta.dirname,
        "..",
        "windows",
        "msix",
        "Package.appxmanifest.template",
      ),
      "utf8",
    );
    const document = new JSDOM(manifestXml, { contentType: "text/xml" }).window
      .document;

    const capabilities = document.getElementsByTagNameNS(
      "http://schemas.microsoft.com/appx/manifest/foundation/windows10",
      "Capabilities",
    )[0];
    expect(capabilities).toBeDefined();

    const capabilityNames = Array.from(capabilities.children).map(
      (capability) =>
        `${capability.namespaceURI}:${capability.localName}:${capability.getAttribute("Name")}`,
    );

    expect(capabilityNames).toContain(
      "http://schemas.microsoft.com/appx/manifest/foundation/windows10:Capability:privateNetworkClientServer",
    );
    expect(capabilityNames).toContain(
      "http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities:Capability:runFullTrust",
    );
  });
});
