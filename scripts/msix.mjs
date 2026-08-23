// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(import.meta.dirname, "..");
const templatePath = resolve(
  root,
  "windows",
  "msix",
  "Package.appxmanifest.template",
);
const assetDirectory = resolve(root, "windows", "msix", "Assets");
const outputDirectory = resolve(root, "target", "msix");
const packageDirectory = resolve(outputDirectory, "package");
const developmentIdentity = Object.freeze({
  name: "GameBuddy.GGanbuAppBridge.Development",
  publisher: "CN=Game Buddy Development",
  publisherDisplayName: "Game Buddy",
});
const requiredAssets = new Map([
  ["StoreLogo.png", [50, 50]],
  ["Square44x44Logo.png", [44, 44]],
  ["Square150x150Logo.png", [150, 150]],
]);
const tokens = Object.freeze({
  name: "__IDENTITY_NAME__",
  publisher: "__PUBLISHER__",
  publisherDisplayName: "__PUBLISHER_DISPLAY_NAME__",
  version: "__MSIX_VERSION__",
});

export function semverToMsixVersion(version) {
  const match =
    /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/.exec(
      version,
    );
  if (!match) {
    throw new Error(`Invalid semantic version: ${version}`);
  }
  if (
    (match[4] ?? "").split(".").some((identifier) => /^0\d+$/.test(identifier))
  ) {
    throw new Error(`Invalid semantic version: ${version}`);
  }
  if (match[4]) {
    throw new Error(
      `Prerelease version ${version} cannot be mapped to a unique MSIX version`,
    );
  }

  const parts = match.slice(1, 4).map(Number);
  const msixParts = [parts[0] + 1, parts[1], parts[2], 0];
  if (msixParts.some((part) => part < 0 || part > 65_535)) {
    throw new Error(
      `Version ${version} cannot be represented as an MSIX version`,
    );
  }

  return msixParts.join(".");
}

export function escapeXml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

export function renderManifest(template, identity, msixVersion) {
  const replacements = new Map([
    [tokens.name, identity.name],
    [tokens.publisher, identity.publisher],
    [tokens.publisherDisplayName, identity.publisherDisplayName],
    [tokens.version, msixVersion],
  ]);
  let manifest = template;
  for (const [token, value] of replacements) {
    if (!manifest.includes(token)) {
      throw new Error(`MSIX manifest template is missing ${token}`);
    }
    manifest = manifest.replaceAll(token, escapeXml(value));
  }
  if (/__[A-Z0-9_]+__/.test(manifest)) {
    throw new Error("MSIX manifest contains an unresolved template token");
  }
  return manifest;
}

function readVersion() {
  return readFileSync(resolve(root, "VERSION"), "utf8").trim();
}

function readIdentity(environment = process.env) {
  const configured = {
    name: environment.MSIX_IDENTITY_NAME?.trim() ?? "",
    publisher: environment.MSIX_PUBLISHER?.trim() ?? "",
    publisherDisplayName: environment.MSIX_PUBLISHER_DISPLAY_NAME?.trim() ?? "",
  };
  const configuredCount = Object.values(configured).filter(Boolean).length;
  if (configuredCount !== 0 && configuredCount !== 3) {
    throw new Error(
      "Set all of MSIX_IDENTITY_NAME, MSIX_PUBLISHER, and MSIX_PUBLISHER_DISPLAY_NAME, or none of them",
    );
  }
  return configuredCount === 3
    ? { identity: configured, storeReady: true }
    : { identity: developmentIdentity, storeReady: false };
}

function readPngDimensions(path) {
  const bytes = readFileSync(path);
  const pngSignature = "89504e470d0a1a0a";
  if (
    bytes.length < 24 ||
    bytes.subarray(0, 8).toString("hex") !== pngSignature
  ) {
    throw new Error(`${relative(root, path)} is not a valid PNG file`);
  }
  return [bytes.readUInt32BE(16), bytes.readUInt32BE(20)];
}

function checkAssets() {
  for (const [filename, expectedDimensions] of requiredAssets) {
    const path = resolve(assetDirectory, filename);
    if (!existsSync(path)) {
      throw new Error(`Missing MSIX asset: ${relative(root, path)}`);
    }
    const actualDimensions = readPngDimensions(path);
    if (actualDimensions.join("x") !== expectedDimensions.join("x")) {
      throw new Error(
        `${relative(root, path)} must be ${expectedDimensions.join("x")}, found ${actualDimensions.join("x")}`,
      );
    }
  }
}

function validatePackageDirectory() {
  const expected = resolve(root, "target", "msix", "package");
  if (
    packageDirectory !== expected ||
    !packageDirectory.startsWith(`${resolve(root, "target")}${sep}`)
  ) {
    throw new Error("Refusing to prepare an unexpected MSIX package directory");
  }
}

function writeGithubOutputs(values) {
  const githubOutput = process.env.GITHUB_OUTPUT;
  if (!githubOutput) return;
  const lines = Object.entries(values).map(([key, value]) => `${key}=${value}`);
  writeFileSync(githubOutput, `${lines.join("\n")}\n`, { flag: "a" });
}

function check() {
  checkAssets();
  const version = readVersion();
  const msixVersion = semverToMsixVersion(version);
  const template = readFileSync(templatePath, "utf8");
  const { identity, storeReady } = readIdentity();
  renderManifest(template, identity, msixVersion);
  console.log(
    `MSIX inputs are valid (${version} -> ${msixVersion}; Store identity ${storeReady ? "configured" : "using development placeholders"})`,
  );
}

function prepare() {
  checkAssets();
  validatePackageDirectory();

  const version = readVersion();
  const msixVersion = semverToMsixVersion(version);
  const { identity, storeReady } = readIdentity();
  const executable = resolve(
    root,
    "src-tauri",
    "target",
    "release",
    "gganbu-app-bridge.exe",
  );
  if (!existsSync(executable)) {
    throw new Error(
      `Windows release executable not found: ${relative(root, executable)}`,
    );
  }

  rmSync(packageDirectory, { recursive: true, force: true });
  mkdirSync(resolve(packageDirectory, "Assets"), { recursive: true });
  copyFileSync(executable, resolve(packageDirectory, basename(executable)));
  for (const filename of requiredAssets.keys()) {
    copyFileSync(
      resolve(assetDirectory, filename),
      resolve(packageDirectory, "Assets", filename),
    );
  }
  for (const filename of ["LICENSE", "THIRD_PARTY_NOTICES.md"]) {
    copyFileSync(resolve(root, filename), resolve(packageDirectory, filename));
  }

  const manifest = renderManifest(
    readFileSync(templatePath, "utf8"),
    identity,
    msixVersion,
  );
  writeFileSync(resolve(packageDirectory, "Package.appxmanifest"), manifest);
  mkdirSync(dirname(resolve(outputDirectory, "submission-metadata.json")), {
    recursive: true,
  });
  const filename = `GGanbu-App-Bridge_${msixVersion}_x64.msix`;
  writeFileSync(
    resolve(outputDirectory, "submission-metadata.json"),
    `${JSON.stringify(
      {
        applicationVersion: version,
        msixVersion,
        identity,
        storeReady,
        note: storeReady
          ? "Identity values were supplied from Partner Center."
          : "Development placeholders were used. Do not submit this package to Partner Center.",
      },
      null,
      2,
    )}\n`,
  );
  writeGithubOutputs({
    identity_configured: storeReady,
    msix_filename: filename,
    msix_version: msixVersion,
  });

  console.log(`Prepared ${relative(root, packageDirectory)}`);
  if (!storeReady) {
    console.warn(
      "Partner Center identity is not configured; this build is for packaging validation only.",
    );
  }
}

function main() {
  const command = process.argv[2] ?? "check";
  if (command === "check") {
    check();
    return;
  }
  if (command === "prepare") {
    prepare();
    return;
  }
  throw new Error(`Unknown MSIX command: ${command}`);
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
