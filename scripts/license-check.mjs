// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const expectedLicenseHash =
  "0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0";
const requiredLicense = "AGPL-3.0-only";

function read(relativePath) {
  return readFileSync(resolve(root, relativePath), "utf8");
}

function fail(message) {
  console.error(message);
  process.exitCode = 1;
}

const licenseHash = createHash("sha256")
  .update(readFileSync(resolve(root, "LICENSE")))
  .digest("hex");
if (licenseHash !== expectedLicenseHash) {
  fail("LICENSE is not the unmodified GNU AGPL version 3 text.");
}

const packageManifest = JSON.parse(read("package.json"));
if (packageManifest.license !== requiredLicense) {
  fail("package.json must declare " + requiredLicense + ".");
}

const cargoManifest = read("src-tauri/Cargo.toml");
if (!/^license = "AGPL-3\.0-only"$/m.test(cargoManifest)) {
  fail("src-tauri/Cargo.toml must declare " + requiredLicense + ".");
}

const tauriConfig = JSON.parse(read("src-tauri/tauri.conf.json"));
if (
  tauriConfig.bundle?.license !== requiredLicense ||
  tauriConfig.bundle?.licenseFile !== "../LICENSE"
) {
  fail("The Tauri bundle must include the AGPL license.");
}

for (const relativePath of [
  "README.md",
  "CONTRIBUTING.md",
  "TRADEMARKS.md",
  "THIRD_PARTY_NOTICES.md",
  "docs/preset-provenance.md",
]) {
  if (!existsSync(resolve(root, relativePath))) {
    fail("Required licensing file is missing: " + relativePath);
  }
}

const tracked = execFileSync(
  "git",
  ["ls-files", "--cached", "--others", "--exclude-standard"],
  { cwd: root, encoding: "utf8" },
)
  .split("\n")
  .filter(Boolean)
  .filter((relativePath) => existsSync(resolve(root, relativePath)));

const excludedFromLegacyScan = new Set([
  "LICENSE",
  "THIRD_PARTY_NOTICES.md",
  "pnpm-lock.yaml",
  "src-tauri/Cargo.lock",
]);
const legacyPattern = new RegExp(
  "(?:licensed under (?:the )?MIT|SPDX-" +
    "License-Identifier:\\s*MIT\\b|^MIT License$)",
  "im",
);

for (const relativePath of tracked) {
  if (
    excludedFromLegacyScan.has(relativePath) ||
    relativePath.startsWith("LICENSES/")
  ) {
    continue;
  }
  const absolutePath = resolve(root, relativePath);
  if (!statSync(absolutePath).isFile()) continue;
  const contents = readFileSync(absolutePath);
  if (contents.includes(0)) continue;
  if (legacyPattern.test(contents.toString("utf8"))) {
    fail("Legacy MIT project declaration remains in " + relativePath + ".");
  }
}

const presetDirectory = resolve(root, "src-tauri/assets/config/hotkeys");
const bundledPresets = existsSync(presetDirectory)
  ? readdirSync(presetDirectory).filter((name) => /\.blkx?$/i.test(name))
  : [];
if (bundledPresets.length === 0) {
  fail("The compatibility preset directory is empty.");
}

const appSource = read("src/App.tsx");
for (const marker of [
  "GNU Affero General Public License",
  "This program comes with no warranty",
  "Get the corresponding source",
]) {
  if (!appSource.includes(marker)) {
    fail("The About view is missing this legal notice: " + marker);
  }
}

if (process.exitCode) process.exit(process.exitCode);
console.log("Project license declarations are consistent with AGPL-3.0-only.");
