// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const versionPath = resolve(root, "VERSION");
const version = process.argv[2] ?? readFileSync(versionPath, "utf8").trim();

const semverPattern =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

function isValidSemver(version) {
  const match = semverPattern.exec(version);
  if (!match) return false;

  return !(match[4] ?? "").split(".").some((identifier) => {
    if (identifier.length < 2 || !identifier.startsWith("0")) return false;
    return [...identifier].every(
      (character) => character >= "0" && character <= "9",
    );
  });
}

if (!isValidSemver(version)) {
  console.error(`Invalid semantic version: ${version}`);
  process.exit(1);
}

function read(relativePath) {
  return readFileSync(resolve(root, relativePath), "utf8").replace(
    /\r\n?/g,
    "\n",
  );
}

function readForPlan(relativePath) {
  try {
    return read(relativePath);
  } catch {
    throw new Error(`${relativePath}: found missing`);
  }
}

function write(relativePath, contents) {
  writeFileSync(resolve(root, relativePath), contents);
}

function planReplacement(relativePath, pattern, replacement) {
  const contents = readForPlan(relativePath);
  if (!pattern.test(contents)) {
    throw new Error(`Could not update version in ${relativePath}`);
  }

  const updated = contents.replace(pattern, replacement);
  return { relativePath, original: contents, updated };
}

function planWrite(relativePath, contents) {
  return {
    relativePath,
    original: readForPlan(relativePath),
    updated: contents,
  };
}

function applyPlans(plans) {
  const applied = [];
  try {
    for (const plan of plans) {
      if (plan.updated !== plan.original) {
        write(plan.relativePath, plan.updated);
        applied.push(plan);
      }
    }
  } catch (error) {
    for (const plan of applied.reverse()) {
      try {
        write(plan.relativePath, plan.original);
      } catch (rollbackError) {
        console.error(
          `Could not roll back ${plan.relativePath}: ${rollbackError}`,
        );
      }
    }
    throw new Error(`Could not update application versions: ${error}`);
  }
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function expectedVersions() {
  const readVersion = (relativePath, extract) => {
    try {
      return extract(read(relativePath));
    } catch {
      return undefined;
    }
  };
  return new Map([
    [
      "package.json",
      readVersion("package.json", (contents) => JSON.parse(contents).version),
    ],
    [
      "src-tauri/Cargo.toml",
      readVersion(
        "src-tauri/Cargo.toml",
        (contents) => contents.match(/^version = "([^"]+)"$/m)?.[1],
      ),
    ],
    [
      "src-tauri/tauri.conf.json",
      readVersion(
        "src-tauri/tauri.conf.json",
        (contents) => JSON.parse(contents).version,
      ),
    ],
    [
      "src/version.ts",
      readVersion(
        "src/version.ts",
        (contents) =>
          contents.match(/^export const APP_VERSION = "([^"]+)";$/m)?.[1],
      ),
    ],
  ]);
}

function checkVersions({ requireChangelog = true } = {}) {
  const sourceVersion = readFileSync(versionPath, "utf8").trim();
  if (!isValidSemver(sourceVersion)) {
    throw new Error(`Invalid semantic version in VERSION: ${sourceVersion}`);
  }

  const versions = expectedVersions();
  const mismatches = [...versions].filter(
    ([, fileVersion]) => fileVersion !== sourceVersion,
  );

  let lockVersion;
  try {
    lockVersion = read("src-tauri/Cargo.lock").match(
      /\[\[package\]\]\r?\nname = "gganbu-app-bridge"\r?\nversion = "([^"]+)"/,
    )?.[1];
  } catch {
    lockVersion = undefined;
  }
  if (lockVersion !== sourceVersion) {
    mismatches.push(["src-tauri/Cargo.lock", lockVersion]);
  }

  if (requireChangelog) {
    const changelogHeading = new RegExp(
      `^## \\[${escapeRegExp(sourceVersion)}\\](?:\\s+-\\s+\\d{4}-\\d{2}-\\d{2})?\\s*$`,
      "m",
    );
    if (!changelogHeading.test(read("CHANGELOG.md"))) {
      mismatches.push([
        "CHANGELOG.md",
        `missing ## [${sourceVersion}] heading`,
      ]);
    }
  }

  if (mismatches.length > 0) {
    for (const [filePath, fileVersion] of mismatches) {
      console.error(
        `${filePath}: expected ${sourceVersion}, found ${fileVersion ?? "missing"}`,
      );
    }
    process.exit(1);
  }

  console.log(`All application versions match ${sourceVersion}`);
}

if (process.argv[2] === undefined) {
  checkVersions();
} else {
  const plans = [
    planWrite("VERSION", `${version}\n`),
    planReplacement(
      "package.json",
      /("version": ")[^"]+(",)/,
      `$1${version}$2`,
    ),
    planReplacement(
      "src-tauri/Cargo.toml",
      /(^version = ")[^"]+("$)/m,
      `$1${version}$2`,
    ),
    planReplacement(
      "src-tauri/tauri.conf.json",
      /("version": ")[^"]+(",)/,
      `$1${version}$2`,
    ),
    planReplacement(
      "src-tauri/Cargo.lock",
      /(\[\[package\]\]\r?\nname = "gganbu-app-bridge"\r?\nversion = ")[^"]+/,
      `$1${version}`,
    ),
    planWrite(
      "src/version.ts",
      `// SPDX-FileCopyrightText: 2026 Game Buddy\n// SPDX-License-Identifier: AGPL-3.0-only\n\n// Generated by pnpm version:set. Do not edit.\nexport const APP_VERSION = "${version}";\n`,
    ),
  ];
  applyPlans(plans);
  console.log(`Updated application version to ${version}`);
  checkVersions({ requireChangelog: false });
}
