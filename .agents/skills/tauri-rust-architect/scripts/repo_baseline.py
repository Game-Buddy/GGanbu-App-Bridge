#!/usr/bin/env python3
"""Generate a heuristic inventory of a Rust or Tauri repository.

This script intentionally uses only the Python standard library. It identifies
project structure, Tauri configuration, capabilities, commands, tests, CI, and
review signals. It does not execute project code and it is not a security
scanner or a substitute for source review.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from collections import Counter
from pathlib import Path
from typing import Any, Iterable

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - depends on interpreter version
    tomllib = None  # type: ignore[assignment]

SCHEMA_VERSION = 1
MAX_TEXT_FILE_BYTES = 2 * 1024 * 1024
MAX_LOCATIONS_PER_SIGNAL = 12

IGNORED_DIRECTORIES = {
    ".git",
    ".hg",
    ".svn",
    ".idea",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".turbo",
    ".venv",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "out",
    "target",
    "vendor",
}

TEXT_SUFFIXES = {
    ".c",
    ".cc",
    ".cpp",
    ".css",
    ".h",
    ".hpp",
    ".html",
    ".js",
    ".json",
    ".json5",
    ".jsx",
    ".md",
    ".mjs",
    ".rs",
    ".scss",
    ".sh",
    ".sql",
    ".svelte",
    ".toml",
    ".ts",
    ".tsx",
    ".vue",
    ".yaml",
    ".yml",
}

KNOWN_TEXT_FILES = {
    "Cargo.lock",
    "Cargo.toml",
    "Dockerfile",
    "Justfile",
    "Makefile",
    "README",
    "rust-toolchain",
}

FRONTEND_SUFFIXES = {".js", ".jsx", ".mjs", ".ts", ".tsx", ".svelte", ".vue"}
TEST_FILE_PATTERNS = (
    re.compile(r"(^|/)(tests?|__tests__)(/|$)", re.IGNORECASE),
    re.compile(r"\.(spec|test)\.(js|jsx|ts|tsx)$", re.IGNORECASE),
)

RUST_SIGNAL_PATTERNS: tuple[tuple[str, str, str, re.Pattern[str]], ...] = (
    (
        "unsafe-rust",
        "review",
        "Unsafe Rust requires a documented invariant and focused audit.",
        re.compile(r"\bunsafe\s+(?:fn|impl|trait|extern|\{)"),
    ),
    (
        "panic-or-unwrap-review",
        "review",
        "unwrap, expect, or panic requires context; confirm it cannot turn external failure into a crash.",
        re.compile(r"\.(?:unwrap|expect)\s*\(|\bpanic!\s*\("),
    ),
    (
        "unfinished-runtime-path",
        "review",
        "todo or unimplemented macros may remain on a reachable path.",
        re.compile(r"\b(?:todo|unimplemented)!\s*\("),
    ),
    (
        "process-execution",
        "security",
        "Process creation or command execution needs strict argument and permission control.",
        re.compile(r"(?:std|tokio)::process::Command::new\s*\(|\bCommand::new\s*\("),
    ),
    (
        "network-listener",
        "security",
        "A local or remote listener creates an authentication and exposure boundary.",
        re.compile(r"TcpListener::bind|HttpServer::new|axum::serve|warp::serve"),
    ),
    (
        "all-interface-bind",
        "security",
        "Binding to all interfaces may expose a service beyond the local machine.",
        re.compile(r"(?:0\.0\.0\.0|\[::\])"),
    ),
    (
        "detached-task",
        "reliability",
        "Spawned tasks need ownership, cancellation, error reporting, and shutdown behavior.",
        re.compile(r"(?:tauri::async_runtime|tokio)::spawn\s*\("),
    ),
)

CONFIG_SIGNAL_PATTERNS: tuple[tuple[str, str, str, re.Pattern[str]], ...] = (
    (
        "disabled-or-null-csp",
        "security",
        "A disabled or null CSP expands the impact of frontend compromise.",
        re.compile(r"[\"']?csp[\"']?\s*[:=]\s*(?:null|false)", re.IGNORECASE),
    ),
    (
        "unsafe-csp-token",
        "security",
        "unsafe-eval or unsafe-inline in CSP needs a narrow, documented justification.",
        re.compile(r"'unsafe-(?:eval|inline)'", re.IGNORECASE),
    ),
    (
        "global-tauri-api",
        "security",
        "Global Tauri API exposure increases the frontend privilege surface.",
        re.compile(
            r"[\"\']?(?:withGlobalTauri|with_global_tauri)[\"\']?"
            r"\s*[:=]\s*true",
            re.IGNORECASE,
        ),
    ),
    (
        "disabled-csp-asset-rewrite",
        "security",
        "Disabling Tauri CSP asset modification needs explicit security review.",
        re.compile(
            r"[\"\']?(?:dangerousDisableAssetCspModification|"
            r"dangerous_disable_asset_csp_modification)[\"\']?"
            r"\s*[:=]\s*true",
            re.IGNORECASE,
        ),
    ),
)


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate a heuristic Rust/Tauri repository inventory."
    )
    parser.add_argument("repository", type=Path, help="Repository root to inspect")
    parser.add_argument(
        "--format",
        choices=("markdown", "json"),
        default="markdown",
        help="Output format (default: markdown)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Write output to this file instead of stdout",
    )
    parser.add_argument(
        "--max-file-bytes",
        type=int,
        default=MAX_TEXT_FILE_BYTES,
        help=f"Maximum text file size to scan (default: {MAX_TEXT_FILE_BYTES})",
    )
    return parser.parse_args(argv)


def relative(path: Path, root: Path) -> str:
    return path.relative_to(root).as_posix()


def iter_repository_files(root: Path) -> Iterable[Path]:
    for current, directories, files in os.walk(root, followlinks=False):
        directories[:] = sorted(
            directory
            for directory in directories
            if directory not in IGNORED_DIRECTORIES
            and not (Path(current) / directory).is_symlink()
        )
        current_path = Path(current)
        for filename in sorted(files):
            path = current_path / filename
            if not path.is_symlink():
                yield path


def is_text_candidate(path: Path) -> bool:
    return path.suffix.lower() in TEXT_SUFFIXES or path.name in KNOWN_TEXT_FILES


def read_text(path: Path, max_bytes: int) -> str | None:
    try:
        if path.stat().st_size > max_bytes:
            return None
        raw = path.read_bytes()
    except OSError:
        return None

    if b"\x00" in raw:
        return None
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        return raw.decode("utf-8", errors="replace")


def load_toml(path: Path) -> tuple[dict[str, Any] | None, str | None]:
    if tomllib is None:
        return None, "Python 3.11 or newer is required to parse TOML"
    try:
        with path.open("rb") as handle:
            value = tomllib.load(handle)
        return value, None
    except (OSError, tomllib.TOMLDecodeError) as exc:
        return None, str(exc)


def load_json(path: Path) -> tuple[dict[str, Any] | None, str | None]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(value, dict):
            return None, "top-level value is not an object"
        return value, None
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        return None, str(exc)


def nested_get(value: Any, *keys: str) -> Any:
    current = value
    for key in keys:
        if not isinstance(current, dict):
            return None
        current = current.get(key)
    return current


def dependency_name_and_version(value: Any) -> str | None:
    if isinstance(value, str):
        return value
    if isinstance(value, dict):
        version = value.get("version")
        if isinstance(version, str):
            return version
        if "path" in value:
            return f"path:{value['path']}"
        if "git" in value:
            return f"git:{value['git']}"
    return None


def collect_dependency_tables(document: dict[str, Any]) -> dict[str, Any]:
    collected: dict[str, Any] = {}
    for key in ("dependencies", "dev-dependencies", "build-dependencies"):
        table = document.get(key)
        if isinstance(table, dict):
            collected.update(table)

    targets = document.get("target")
    if isinstance(targets, dict):
        for target_config in targets.values():
            if not isinstance(target_config, dict):
                continue
            for key in ("dependencies", "dev-dependencies", "build-dependencies"):
                table = target_config.get(key)
                if isinstance(table, dict):
                    collected.update(table)
    return collected


def inspect_cargo_manifest(path: Path, root: Path) -> dict[str, Any]:
    document, error = load_toml(path)
    result: dict[str, Any] = {"path": relative(path, root)}
    if error or document is None:
        result["parse_error"] = error
        return result

    package = document.get("package") if isinstance(document.get("package"), dict) else {}
    workspace = document.get("workspace") if isinstance(document.get("workspace"), dict) else {}
    dependencies = collect_dependency_tables(document)
    tauri_dependencies = {
        name: dependency_name_and_version(value)
        for name, value in sorted(dependencies.items())
        if name == "tauri" or name == "tauri-build" or name.startswith("tauri-plugin-")
    }

    result.update(
        {
            "package": package.get("name"),
            "version": package.get("version"),
            "edition": package.get("edition"),
            "rust_version": package.get("rust-version"),
            "workspace_members": workspace.get("members", []),
            "direct_dependency_count": len(dependencies),
            "tauri_dependencies": tauri_dependencies,
        }
    )
    return result


def normalize_list(value: Any) -> list[Any]:
    if value is None:
        return []
    if isinstance(value, list):
        return value
    return [value]


def permission_identifier(value: Any) -> str:
    if isinstance(value, str):
        return value
    if isinstance(value, dict):
        identifier = value.get("identifier")
        if isinstance(identifier, str):
            return identifier
    return "<structured-permission>"


def inspect_capability(path: Path, root: Path) -> dict[str, Any]:
    suffix = path.suffix.lower()
    if suffix == ".toml":
        document, error = load_toml(path)
    elif suffix == ".json":
        document, error = load_json(path)
    else:
        document, error = None, "JSON5 is scanned as text but not parsed"

    result: dict[str, Any] = {"path": relative(path, root)}
    if error or document is None:
        result["parse_error"] = error
        return result

    permissions_raw = normalize_list(document.get("permissions"))
    permissions = [permission_identifier(value) for value in permissions_raw]
    windows = [str(value) for value in normalize_list(document.get("windows"))]
    webviews = [str(value) for value in normalize_list(document.get("webviews"))]
    platforms = [str(value) for value in normalize_list(document.get("platforms"))]
    remote_urls = [
        str(value)
        for value in normalize_list(nested_get(document, "remote", "urls"))
    ]

    broad_permissions = [
        permission
        for permission in permissions
        if permission == "*"
        or permission.endswith(":*")
        or permission in {"fs:default", "shell:default", "http:default", "process:default"}
    ]

    result.update(
        {
            "identifier": document.get("identifier"),
            "windows": windows,
            "webviews": webviews,
            "platforms": platforms,
            "permissions": permissions,
            "remote_urls": remote_urls,
            "wildcard_window_or_webview": "*" in windows or "*" in webviews,
            "broad_permission_signals": broad_permissions,
        }
    )
    return result


def inspect_tauri_config(path: Path, root: Path) -> dict[str, Any]:
    suffix = path.suffix.lower()
    if suffix == ".toml":
        document, error = load_toml(path)
    elif suffix == ".json":
        document, error = load_json(path)
    else:
        document, error = None, "JSON5 is scanned as text but not parsed"

    result: dict[str, Any] = {"path": relative(path, root)}
    if document is None:
        if error:
            result["parse_note"] = error
        return result

    security = nested_get(document, "app", "security")
    if not isinstance(security, dict):
        security = {}
    build = document.get("build") if isinstance(document.get("build"), dict) else {}

    csp_present = "csp" in security
    csp_value = security.get("csp")
    result.update(
        {
            "product_name": document.get("productName"),
            "version": document.get("version"),
            "identifier": document.get("identifier"),
            "dev_url": build.get("devUrl"),
            "frontend_dist": build.get("frontendDist"),
            "capabilities": security.get("capabilities"),
            "capabilities_present": "capabilities" in security,
            "csp_present": csp_present,
            "csp_disabled_or_null": csp_present and csp_value in (None, False),
            "with_global_tauri": nested_get(document, "app", "withGlobalTauri") is True,
            "dangerous_disable_asset_csp_modification": security.get(
                "dangerousDisableAssetCspModification"
            )
            is True,
        }
    )
    return result


def line_number(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def add_signal(
    signals: dict[str, dict[str, Any]],
    signal_id: str,
    category: str,
    message: str,
    location: str,
) -> None:
    signal = signals.setdefault(
        signal_id,
        {
            "id": signal_id,
            "category": category,
            "message": message,
            "count": 0,
            "locations": [],
        },
    )
    signal["count"] += 1
    if len(signal["locations"]) < MAX_LOCATIONS_PER_SIGNAL:
        signal["locations"].append(location)


def scan_patterns(
    path: Path,
    root: Path,
    text: str,
    patterns: tuple[tuple[str, str, str, re.Pattern[str]], ...],
    signals: dict[str, dict[str, Any]],
) -> None:
    rel = relative(path, root)
    for signal_id, category, message, pattern in patterns:
        for match in pattern.finditer(text):
            add_signal(
                signals,
                signal_id,
                category,
                message,
                f"{rel}:{line_number(text, match.start())}",
            )


def extract_tauri_commands(path: Path, root: Path, text: str) -> list[dict[str, Any]]:
    command_pattern = re.compile(
        r"#\s*\[\s*tauri::command(?:\s*\([^\]]*\))?\s*\]"
        r"(?P<body>.{0,800}?)"
        r"\b(?:pub(?:\s*\([^)]*\))?\s+)?(?:async\s+)?fn\s+"
        r"(?P<name>[A-Za-z_][A-Za-z0-9_]*)",
        re.DOTALL,
    )
    commands: list[dict[str, Any]] = []
    for match in command_pattern.finditer(text):
        commands.append(
            {
                "name": match.group("name"),
                "path": relative(path, root),
                "line": line_number(text, match.start()),
            }
        )
    return commands


def is_test_file(relative_path: str) -> bool:
    return any(pattern.search(relative_path) for pattern in TEST_FILE_PATTERNS)


def inspect_repository(root: Path, max_file_bytes: int) -> dict[str, Any]:
    files = list(iter_repository_files(root))
    text_files: dict[Path, str] = {}
    skipped_large_or_binary = 0

    suffix_counts: Counter[str] = Counter()
    for path in files:
        suffix_counts[path.suffix.lower() or "<none>"] += 1
        if not is_text_candidate(path):
            continue
        text = read_text(path, max_file_bytes)
        if text is None:
            skipped_large_or_binary += 1
            continue
        text_files[path] = text

    cargo_paths = sorted(path for path in files if path.name == "Cargo.toml")
    cargo_manifests = [inspect_cargo_manifest(path, root) for path in cargo_paths]

    tauri_config_paths = sorted(
        path
        for path in files
        if path.name in {"tauri.conf.json", "tauri.conf.json5", "tauri.conf.toml"}
    )
    tauri_configs = [inspect_tauri_config(path, root) for path in tauri_config_paths]

    capability_paths = sorted(
        path
        for path in files
        if "capabilities" in path.parts and path.suffix.lower() in {".json", ".json5", ".toml"}
    )
    capabilities = [inspect_capability(path, root) for path in capability_paths]

    commands: list[dict[str, Any]] = []
    signals: dict[str, dict[str, Any]] = {}
    rust_test_attributes = 0
    frontend_test_files: list[str] = []

    for path, text in text_files.items():
        rel = relative(path, root)
        if path.suffix.lower() == ".rs":
            commands.extend(extract_tauri_commands(path, root, text))
            rust_test_attributes += len(
                re.findall(r"#\s*\[\s*(?:tokio::)?test(?:\s*\([^\]]*\))?\s*\]", text)
            )
            rust_patterns = RUST_SIGNAL_PATTERNS
            if path.name == "build.rs" or is_test_file(rel):
                rust_patterns = tuple(
                    pattern for pattern in rust_patterns if pattern[0] != "panic-or-unwrap-review"
                )
            scan_patterns(path, root, text, rust_patterns, signals)
        if path in tauri_config_paths or path in capability_paths:
            scan_patterns(path, root, text, CONFIG_SIGNAL_PATTERNS, signals)
        if path.suffix.lower() in FRONTEND_SUFFIXES and is_test_file(rel):
            frontend_test_files.append(rel)

    for capability in capabilities:
        path = capability.get("path", "<unknown>")
        if capability.get("wildcard_window_or_webview"):
            add_signal(
                signals,
                "wildcard-capability-target",
                "security",
                "A capability targets every window or WebView and may merge privilege boundaries.",
                path,
            )
        for permission in capability.get("broad_permission_signals", []):
            add_signal(
                signals,
                "broad-capability-permission",
                "security",
                "A broad or default sensitive permission needs scope and least-privilege review.",
                f"{path} ({permission})",
            )
        if capability.get("remote_urls"):
            add_signal(
                signals,
                "remote-capability-access",
                "security",
                "Remote content can access Tauri permissions through this capability.",
                path,
            )


    parsed_tauri_configs = [
        config for config in tauri_configs if "parse_note" not in config
    ]
    for config in parsed_tauri_configs:
        if not config.get("csp_present"):
            add_signal(
                signals,
                "missing-csp",
                "security",
                "No CSP is configured in this Tauri config; verify an equivalent reviewed policy exists.",
                config.get("path", "<unknown>"),
            )

    if capabilities and parsed_tauri_configs and not any(
        config.get("capabilities_present") for config in parsed_tauri_configs
    ):
        add_signal(
            signals,
            "implicit-capability-enable",
            "security",
            "Capability files exist but are not explicitly enumerated in parsed Tauri configuration.",
            parsed_tauri_configs[0].get("path", "<unknown>"),
        )

    tauri_v2_detected = bool(capabilities) or any(
        str(manifest.get("tauri_dependencies", {}).get("tauri", "")).lstrip("^~=>< ").startswith("2")
        for manifest in cargo_manifests
    )
    custom_command_manifest = any(
        "AppManifest::new" in source and re.search(r"\.commands\s*\(", source)
        for source in text_files.values()
    )
    if tauri_v2_detected and commands and not custom_command_manifest:
        add_signal(
            signals,
            "custom-command-scope-review",
            "security",
            "Registered Tauri application commands need explicit window and permission scoping review.",
            commands[0]["path"],
        )

    plugin_versions: dict[str, list[str]] = {}
    for manifest in cargo_manifests:
        for name, version in manifest.get("tauri_dependencies", {}).items():
            plugin_versions.setdefault(name, [])
            normalized = version or "<unspecified>"
            if normalized not in plugin_versions[name]:
                plugin_versions[name].append(normalized)

    ci_workflows = sorted(
        relative(path, root)
        for path in files
        if len(path.parts) >= 3
        and ".github" in path.parts
        and "workflows" in path.parts
        and path.suffix.lower() in {".yml", ".yaml"}
    )

    documentation_files = sorted(
        relative(path, root)
        for path in files
        if path.suffix.lower() == ".md"
        and (
            path.name.lower().startswith("readme")
            or "docs" in {part.lower() for part in path.parts}
            or "adr" in path.name.lower()
            or "architecture" in path.name.lower()
            or "security" in path.name.lower()
        )
    )

    package_managers = {
        "npm": any(path.name == "package-lock.json" for path in files),
        "pnpm": any(path.name in {"pnpm-lock.yaml", "pnpm-workspace.yaml"} for path in files),
        "yarn": any(path.name == "yarn.lock" for path in files),
        "bun": any(path.name in {"bun.lock", "bun.lockb"} for path in files),
    }

    rust_files = [path for path in files if path.suffix.lower() == ".rs"]
    frontend_files = [path for path in files if path.suffix.lower() in FRONTEND_SUFFIXES]

    result = {
        "schema_version": SCHEMA_VERSION,
        "notice": (
            "Heuristic inventory only. Confirm signals in source and configuration; "
            "this is not a security audit or proof of correctness."
        ),
        "repository": str(root),
        "summary": {
            "total_files": len(files),
            "scanned_text_files": len(text_files),
            "skipped_large_or_binary_text_candidates": skipped_large_or_binary,
            "rust_files": len(rust_files),
            "frontend_files": len(frontend_files),
            "cargo_manifests": len(cargo_manifests),
            "tauri_configs": len(tauri_configs),
            "capability_files": len(capabilities),
            "tauri_commands": len(commands),
            "review_signals": sum(signal["count"] for signal in signals.values()),
        },
        "tooling": {
            "rust_toolchain_files": sorted(
                relative(path, root)
                for path in files
                if path.name in {"rust-toolchain", "rust-toolchain.toml"}
            ),
            "cargo_lock_present": any(path.name == "Cargo.lock" for path in files),
            "deny_toml_present": any(path.name == "deny.toml" for path in files),
            "package_managers": [name for name, present in package_managers.items() if present],
        },
        "cargo_manifests": cargo_manifests,
        "tauri": {
            "dependencies": plugin_versions,
            "configs": tauri_configs,
            "capabilities": capabilities,
            "commands": sorted(commands, key=lambda item: (item["path"], item["line"])),
        },
        "tests": {
            "rust_test_attributes": rust_test_attributes,
            "frontend_test_files": sorted(frontend_test_files),
        },
        "ci_workflows": ci_workflows,
        "documentation_files": documentation_files,
        "review_signals": sorted(
            signals.values(),
            key=lambda item: (
                {"security": 0, "reliability": 1, "review": 2}.get(item["category"], 9),
                item["id"],
            ),
        ),
        "file_suffix_counts": dict(sorted(suffix_counts.items())),
    }
    return result


def markdown_cell(value: Any) -> str:
    if value is None or value == "":
        return "-"
    if isinstance(value, bool):
        return "yes" if value else "no"
    if isinstance(value, list):
        return ", ".join(str(item) for item in value) or "-"
    if isinstance(value, dict):
        return ", ".join(f"{key}={item}" for key, item in value.items()) or "-"
    return str(value).replace("|", "\\|")


def render_markdown(report: dict[str, Any]) -> str:
    summary = report["summary"]
    lines = [
        "# Rust and Tauri Repository Baseline",
        "",
        f"Repository: `{report['repository']}`",
        "",
        f"> {report['notice']}",
        "",
        "## Summary",
        "",
        "| Item | Count |",
        "| --- | ---: |",
    ]
    for key in (
        "total_files",
        "scanned_text_files",
        "rust_files",
        "frontend_files",
        "cargo_manifests",
        "tauri_configs",
        "capability_files",
        "tauri_commands",
        "review_signals",
    ):
        lines.append(f"| {key.replace('_', ' ').title()} | {summary[key]} |")

    lines.extend(["", "## Tooling", ""])
    tooling = report["tooling"]
    lines.append(f"- Rust toolchain files: {markdown_cell(tooling['rust_toolchain_files'])}")
    lines.append(f"- Cargo.lock present: {markdown_cell(tooling['cargo_lock_present'])}")
    lines.append(f"- deny.toml present: {markdown_cell(tooling['deny_toml_present'])}")
    lines.append(f"- Frontend package managers: {markdown_cell(tooling['package_managers'])}")

    lines.extend(["", "## Cargo Manifests", ""])
    if report["cargo_manifests"]:
        lines.extend(
            [
                "| Path | Package | Edition | Rust Version | Direct Dependencies | Tauri Dependencies |",
                "| --- | --- | --- | --- | ---: | --- |",
            ]
        )
        for manifest in report["cargo_manifests"]:
            lines.append(
                "| {path} | {package} | {edition} | {rust_version} | {dependency_count} | {tauri} |".format(
                    path=markdown_cell(manifest.get("path")),
                    package=markdown_cell(manifest.get("package")),
                    edition=markdown_cell(manifest.get("edition")),
                    rust_version=markdown_cell(manifest.get("rust_version")),
                    dependency_count=markdown_cell(manifest.get("direct_dependency_count")),
                    tauri=markdown_cell(manifest.get("tauri_dependencies", {})),
                )
            )
    else:
        lines.append("No Cargo manifests found.")

    lines.extend(["", "## Tauri Configuration", ""])
    configs = report["tauri"]["configs"]
    if configs:
        lines.extend(
            [
                "| Path | Identifier | CSP Present | CSP Disabled | Global API | Capabilities |",
                "| --- | --- | --- | --- | --- | --- |",
            ]
        )
        for config in configs:
            lines.append(
                "| {path} | {identifier} | {csp} | {disabled} | {global_api} | {capabilities} |".format(
                    path=markdown_cell(config.get("path")),
                    identifier=markdown_cell(config.get("identifier")),
                    csp=markdown_cell(config.get("csp_present")),
                    disabled=markdown_cell(config.get("csp_disabled_or_null")),
                    global_api=markdown_cell(config.get("with_global_tauri")),
                    capabilities=markdown_cell(config.get("capabilities")),
                )
            )
    else:
        lines.append("No Tauri configuration found.")

    lines.extend(["", "## Capabilities", ""])
    capabilities = report["tauri"]["capabilities"]
    if capabilities:
        lines.extend(
            [
                "| Path | Identifier | Windows/WebViews | Platforms | Permissions | Remote URLs |",
                "| --- | --- | --- | --- | --- | --- |",
            ]
        )
        for capability in capabilities:
            targets = capability.get("windows", []) + capability.get("webviews", [])
            lines.append(
                "| {path} | {identifier} | {targets} | {platforms} | {permissions} | {remote} |".format(
                    path=markdown_cell(capability.get("path")),
                    identifier=markdown_cell(capability.get("identifier")),
                    targets=markdown_cell(targets),
                    platforms=markdown_cell(capability.get("platforms")),
                    permissions=markdown_cell(capability.get("permissions")),
                    remote=markdown_cell(capability.get("remote_urls")),
                )
            )
    else:
        lines.append("No capability files found.")

    lines.extend(["", "## Tauri Commands", ""])
    commands = report["tauri"]["commands"]
    if commands:
        for command in commands:
            lines.append(f"- `{command['name']}` at `{command['path']}:{command['line']}`")
    else:
        lines.append("No `#[tauri::command]` functions detected.")

    lines.extend(["", "## Tests, CI, and Documentation", ""])
    tests = report["tests"]
    lines.append(f"- Rust test attributes: {tests['rust_test_attributes']}")
    lines.append(f"- Frontend test files: {markdown_cell(tests['frontend_test_files'])}")
    lines.append(f"- CI workflows: {markdown_cell(report['ci_workflows'])}")
    lines.append(f"- Relevant documentation: {markdown_cell(report['documentation_files'])}")

    lines.extend(["", "## Review Signals", ""])
    signals = report["review_signals"]
    if signals:
        for signal in signals:
            lines.append(
                f"### {signal['id']} ({signal['category']}, {signal['count']} occurrence(s))"
            )
            lines.append("")
            lines.append(signal["message"])
            lines.append("")
            for location in signal["locations"]:
                lines.append(f"- `{location}`")
            if signal["count"] > len(signal["locations"]):
                omitted = signal["count"] - len(signal["locations"])
                lines.append(f"- ... {omitted} additional occurrence(s) omitted")
            lines.append("")
    else:
        lines.append("No configured heuristic review signals were detected.")

    return "\n".join(lines).rstrip() + "\n"


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv or sys.argv[1:])
    root = args.repository.expanduser().resolve()
    if not root.exists():
        print(f"error: repository does not exist: {root}", file=sys.stderr)
        return 2
    if not root.is_dir():
        print(f"error: repository is not a directory: {root}", file=sys.stderr)
        return 2
    if args.max_file_bytes <= 0:
        print("error: --max-file-bytes must be greater than zero", file=sys.stderr)
        return 2

    report = inspect_repository(root, args.max_file_bytes)
    if args.format == "json":
        output = json.dumps(report, indent=2, sort_keys=True) + "\n"
    else:
        output = render_markdown(report)

    if args.output:
        output_path = args.output.expanduser().resolve()
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(output, encoding="utf-8")
    else:
        sys.stdout.write(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
