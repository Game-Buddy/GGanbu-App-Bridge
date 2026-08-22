// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import {
  existsSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const outputPath = resolve(root, "THIRD_PARTY_NOTICES.md");
const checkOnly = process.argv.includes("--check");
const noticeNamePattern =
  /^(?:licen[cs]e|copying|notice|copyright)(?:[._-].*)?$/i;

function run(command, args) {
  return execFileSync(command, args, {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
  });
}

function escapeTable(value) {
  return String(value ?? "")
    .replaceAll("|", "\\|")
    .replaceAll("\n", " ");
}

function escapeHtml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function localNoticeFiles(packageDirectory) {
  if (!existsSync(packageDirectory)) return [];
  return readdirSync(packageDirectory)
    .filter((name) => noticeNamePattern.test(name))
    .map((name) => resolve(packageDirectory, name))
    .filter((path) => statSync(path).isFile())
    .filter((path) => statSync(path).size <= 1024 * 1024)
    .filter((path) => !readFileSync(path).includes(0))
    .sort();
}

function hasIncompatibleSecondaryLicenseNotice(packageDirectory) {
  const pending = [packageDirectory];
  const exhibitBPattern =
    /This Source Code Form is\s+"Incompatible With Secondary Licenses"/;

  while (pending.length > 0) {
    const current = pending.pop();
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const path = resolve(current, entry.name);
      if (entry.isDirectory()) {
        pending.push(path);
        continue;
      }
      if (!entry.isFile() || noticeNamePattern.test(entry.name)) continue;
      if (statSync(path).size > 1024 * 1024) continue;
      const contents = readFileSync(path);
      if (contents.includes(0)) continue;
      if (exhibitBPattern.test(contents.toString("utf8"))) return true;
    }
  }

  return false;
}

const records = [];
const cargoMetadata = JSON.parse(
  run("cargo", [
    "metadata",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--format-version",
    "1",
    "--locked",
    "--offline",
  ]),
);
const projectManifest = resolve(root, "src-tauri/Cargo.toml");

for (const packageEntry of cargoMetadata.packages) {
  if (resolve(packageEntry.manifest_path) === projectManifest) continue;
  const packageDirectory = dirname(packageEntry.manifest_path);
  if (
    packageEntry.license === "MPL-2.0" &&
    hasIncompatibleSecondaryLicenseNotice(packageDirectory)
  ) {
    throw new Error(
      packageEntry.name +
        " " +
        packageEntry.version +
        " is MPL-2.0 Incompatible With Secondary Licenses and cannot be included in this AGPL distribution.",
    );
  }
  records.push({
    ecosystem: "Cargo",
    name: packageEntry.name,
    version: packageEntry.version,
    license: packageEntry.license ?? "NOASSERTION",
    source:
      packageEntry.repository ??
      packageEntry.homepage ??
      packageEntry.source ??
      "Not declared",
    directory: packageDirectory,
  });
}

const pnpmLicenses = JSON.parse(
  run("pnpm", ["licenses", "list", "--prod", "--json", "--long"]),
);
for (const [license, packages] of Object.entries(pnpmLicenses)) {
  for (const packageEntry of packages) {
    for (const version of packageEntry.versions) {
      records.push({
        ecosystem: "pnpm",
        name: packageEntry.name,
        version,
        license,
        source:
          packageEntry.repository ?? packageEntry.homepage ?? "Not declared",
        directory: packageEntry.paths[0],
      });
    }
  }
}

records.sort((left, right) =>
  [left.ecosystem, left.name, left.version]
    .join("\0")
    .localeCompare([right.ecosystem, right.name, right.version].join("\0")),
);

const texts = new Map();
const missingNotices = [];
for (const record of records) {
  const packageLabel =
    record.ecosystem + ": " + record.name + " " + record.version;
  const files = localNoticeFiles(record.directory);
  if (files.length === 0) missingNotices.push(packageLabel);
  for (const file of files) {
    const contents = readFileSync(file, "utf8").replaceAll("\r\n", "\n").trim();
    const hash = createHash("sha256").update(contents).digest("hex");
    const entry = texts.get(hash) ?? {
      contents,
      names: new Set(),
      packages: new Set(),
    };
    entry.names.add(file.split(/[\\/]/).at(-1));
    entry.packages.add(packageLabel);
    texts.set(hash, entry);
  }
}

const lines = [
  "<!-- SPDX-FileCopyrightText: 2026 Game Buddy -->",
  "<!-- SPDX-" + "License-Identifier: AGPL-3.0-only -->",
  "",
  "# Third-party notices",
  "",
  "This file records notices shipped with the locked production dependency graph. The generator copies notice and license files from the locally installed packages without changing their text.",
  "",
  "Regenerate it with pnpm notices:generate. Third-party material remains under the terms shown below.",
  "",
  "## War Thunder compatibility data",
  "",
  "The bundled `src-tauri/assets/config/hotkeys/` files are third-party War Thunder compatibility data credited to [War-Thunder-Datamine](https://github.com/gszabi99/War-Thunder-Datamine). They are not covered by the project AGPL license. See [the provenance disclaimer](docs/preset-provenance.md) for details.",
  "",
  "<!-- REUSE-IgnoreStart -->",
  "",
  "## Package index",
  "",
  "| Ecosystem | Package | Version | Declared license | Source |",
  "| --- | --- | --- | --- | --- |",
];

for (const record of records) {
  lines.push(
    "| " +
      escapeTable(record.ecosystem) +
      " | " +
      escapeTable(record.name) +
      " | " +
      escapeTable(record.version) +
      " | " +
      escapeTable(record.license) +
      " | " +
      escapeTable(record.source) +
      " |",
  );
}

if (missingNotices.length > 0) {
  lines.push(
    "",
    "## Packages without a local notice file",
    "",
    "These packages declare a license in their package metadata, but their installed package directory does not contain a top-level license or notice file:",
    "",
    ...missingNotices.map((name) => "- " + name),
  );
}

lines.push("", "## Included license and notice texts", "");
for (const [hash, entry] of [...texts].sort(([left], [right]) =>
  left.localeCompare(right),
)) {
  const packages = [...entry.packages].sort();
  const countLabel =
    packages.length +
    " package" +
    (packages.length === 1 ? "" : "s") +
    " · SHA-256 " +
    hash;
  lines.push(
    "<details>",
    "<summary>" +
      escapeHtml([...entry.names].sort().join(", ")) +
      " for " +
      countLabel +
      "</summary>",
    "",
    packages.map((name) => "- " + name).join("\n"),
    "",
    "<pre>",
    escapeHtml(entry.contents),
    "</pre>",
    "",
    "</details>",
    "",
  );
}
lines.push("<!-- REUSE-IgnoreEnd -->", "");

const generated = lines.join("\n");
if (checkOnly) {
  if (
    !existsSync(outputPath) ||
    readFileSync(outputPath, "utf8") !== generated
  ) {
    console.error(
      "THIRD_PARTY_NOTICES.md is stale. Run pnpm notices:generate.",
    );
    process.exit(1);
  }
  console.log("THIRD_PARTY_NOTICES.md matches the locked dependencies.");
} else {
  writeFileSync(outputPath, generated);
  console.log(
    "Wrote THIRD_PARTY_NOTICES.md for " +
      records.length +
      " packages and " +
      texts.size +
      " unique notice texts.",
  );
}
