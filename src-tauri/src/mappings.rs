// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::OnceLock,
};

use rust_embed::RustEmbed;
use serde::Serialize;
use tauri::Manager;

use crate::actions;

const DEFAULT_KEYBINDINGS: &str = "config/hotkeys/hotkey.keyboard_shooter_ver3.blkx";
const MAX_KEYBINDING_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_KEYBINDING_FILES: usize = 128;
const MAX_KEYBINDING_DEPTH: usize = 32;
const MAX_KEYBINDINGS: usize = 20_000;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct EmbeddedAssets;

static ASSET_ROOT: OnceLock<PathBuf> = OnceLock::new();

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KeybindingPreview {
    source: String,
    output: String,
    files: Vec<String>,
    bindings: usize,
    hotkeys: Vec<KeybindingRow>,
}

#[derive(Clone, Debug, Serialize)]
struct KeybindingRow {
    action: String,
    binding: String,
}

fn parse_blk_value(raw: &str) -> serde_json::Value {
    let value = raw.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        return serde_json::Value::String(value[1..value.len() - 1].replace("\\\"", "\""));
    }
    value
        .parse::<i64>()
        .map(serde_json::Value::from)
        .unwrap_or_else(|_| serde_json::Value::String(value.to_owned()))
}

fn insert_blk_value(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: String,
    value: serde_json::Value,
) {
    match object.remove(&key) {
        None => {
            object.insert(key, value);
        }
        Some(serde_json::Value::Array(mut values)) => {
            values.push(value);
            object.insert(key, serde_json::Value::Array(values));
        }
        Some(previous) => {
            object.insert(key, serde_json::Value::Array(vec![previous, value]));
        }
    }
}

fn parse_blk(source: &str) -> Result<serde_json::Value, String> {
    let mut stack: Vec<(String, serde_json::Map<String, serde_json::Value>)> =
        vec![(String::new(), serde_json::Map::new())];
    for raw_line in source.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if let Some(stripped) = line.strip_suffix("{}") {
            let key = stripped.trim().to_owned();
            insert_blk_value(
                &mut stack.last_mut().ok_or("invalid block")?.1,
                key,
                serde_json::json!({}),
            );
        } else if let Some(stripped) = line.strip_suffix('{') {
            let key = stripped.trim().to_owned();
            stack.push((key, serde_json::Map::new()));
        } else if line == "}" {
            let (key, child) = stack.pop().ok_or("unexpected closing brace")?;
            insert_blk_value(
                &mut stack.last_mut().ok_or("unexpected block end")?.1,
                key,
                serde_json::Value::Object(child),
            );
        } else if let Some((left, raw_value)) = line.split_once('=') {
            let key = left
                .trim()
                .split(':')
                .next()
                .unwrap_or(left.trim())
                .to_owned();
            insert_blk_value(
                &mut stack.last_mut().ok_or("invalid value")?.1,
                key,
                parse_blk_value(raw_value),
            );
        }
    }
    if stack.len() != 1 {
        return Err("unterminated .blk block".to_owned());
    }
    Ok(serde_json::Value::Object(stack.pop().unwrap().1))
}

fn resolve_case_insensitive(path: &Path) -> std::io::Result<PathBuf> {
    if path.exists() {
        return Ok(path.to_path_buf());
    }

    let file_name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("path '{}' does not exist", path.display()),
        )
    })?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let resolved_parent = resolve_case_insensitive(parent)?;
    let wanted = file_name.to_string_lossy();
    let entry = fs::read_dir(&resolved_parent)?
        .filter_map(Result::ok)
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(&wanted)
        })
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("path '{}' does not exist", path.display()),
            )
        })?;
    Ok(entry.path())
}

fn resolve_base_preset(path: &Path, base: &str) -> Result<PathBuf, String> {
    let candidate = Path::new(base);
    if candidate.is_absolute()
        || candidate
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(format!("unsupported basePresetPaths entry '{base}'"));
    }

    let root = if base.starts_with("config/") {
        path.ancestors()
            .find(|candidate| candidate.file_name().is_some_and(|name| name == "assets"))
            .map(Path::to_path_buf)
            .unwrap_or_else(|| {
                ASSET_ROOT
                    .get()
                    .cloned()
                    .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"))
            })
    } else {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    };
    let mut target = root.join(candidate);
    // War Thunder presets sometimes reference the legacy .blk suffix while
    // the distributed asset is stored as JSON with a .blkx suffix.
    if resolve_case_insensitive(&target).is_err()
        && target
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("blk"))
    {
        target.set_extension("blkx");
    }

    let resolved = resolve_case_insensitive(&target)
        .and_then(fs::canonicalize)
        .map_err(|error| format!("could not resolve basePresetPaths entry '{base}': {error}"))?;
    let canonical_root = fs::canonicalize(&root)
        .map_err(|error| format!("could not resolve preset root for '{base}': {error}"))?;
    if !resolved.starts_with(&canonical_root) {
        return Err(format!(
            "basePresetPaths entry '{base}' escapes its preset root"
        ));
    }
    Ok(resolved)
}

fn load_keybinding_chain(
    path: &Path,
    files: &mut Vec<String>,
    seen: &mut Vec<PathBuf>,
    merged: &mut serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    if seen.len() >= MAX_KEYBINDING_DEPTH {
        return Err(format!(
            "keybinding preset chain exceeds the maximum depth of {MAX_KEYBINDING_DEPTH}"
        ));
    }
    if files.len() >= MAX_KEYBINDING_FILES {
        return Err(format!(
            "keybinding preset chain exceeds the maximum of {MAX_KEYBINDING_FILES} files"
        ));
    }
    let path = resolve_case_insensitive(path)
        .and_then(fs::canonicalize)
        .map_err(|e| format!("could not read '{}': {e}", path.display()))?;
    if seen.contains(&path) {
        return Err(format!(
            "cyclic basePresetPaths reference at '{}'",
            path.display()
        ));
    }
    seen.push(path.clone());
    let mut source = String::new();
    let bytes_read = fs::File::open(&path)
        .and_then(|file| {
            file.take(MAX_KEYBINDING_FILE_BYTES + 1)
                .read_to_string(&mut source)
        })
        .map_err(|e| format!("could not read '{}': {e}", path.display()))?;
    if bytes_read as u64 > MAX_KEYBINDING_FILE_BYTES {
        return Err(format!(
            "keybinding file '{}' exceeds the {MAX_KEYBINDING_FILE_BYTES}-byte limit",
            path.display()
        ));
    }
    let document = if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("blk"))
    {
        parse_blk(&source).map_err(|e| format!("invalid .blk file '{}': {e}", path.display()))?
    } else {
        serde_json::from_str(&source)
            .map_err(|e| format!("invalid JSON in '{}': {e}", path.display()))?
    };
    let controls = document
        .get("controls")
        .ok_or("mapping file is missing controls")?;
    if let Some(bases) = controls
        .get("basePresetPaths")
        .and_then(serde_json::Value::as_object)
    {
        for base in bases.values().filter_map(serde_json::Value::as_str) {
            let parent = resolve_base_preset(&path, base)?;
            load_keybinding_chain(&parent, files, seen, merged)?;
        }
    }
    if let Some(hotkeys) = controls
        .get("hotkeys")
        .and_then(serde_json::Value::as_object)
    {
        for (key, value) in hotkeys {
            if !merged.contains_key(key) && merged.len() >= MAX_KEYBINDINGS {
                return Err(format!(
                    "keybinding preset exceeds the maximum of {MAX_KEYBINDINGS} bindings"
                ));
            }
            merged.insert(key.clone(), value.clone());
        }
    }
    files.push(path.display().to_string());
    seen.pop();
    Ok(())
}

pub(crate) fn load_keybindings_from_path(
    app: &tauri::AppHandle,
    path: Option<PathBuf>,
) -> Result<KeybindingPreview, String> {
    let embedded_root = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("assets");
    for asset in EmbeddedAssets::iter() {
        let target = embedded_root.join(asset.as_ref());
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if !target.exists()
            && let Some(contents) = EmbeddedAssets::get(asset.as_ref())
        {
            fs::write(&target, contents.data.as_ref()).map_err(|e| e.to_string())?;
        }
    }
    let _ = ASSET_ROOT.set(embedded_root.clone());
    let source = match path {
        Some(selected) => {
            if !selected.extension().is_some_and(|extension| {
                extension.eq_ignore_ascii_case("blk") || extension.eq_ignore_ascii_case("blkx")
            }) {
                return Err("only .blk and .blkx files can be selected".to_owned());
            }
            selected
        }
        None => {
            let packaged = app
                .path()
                .resource_dir()
                .map_err(|e| e.to_string())?
                .join("assets")
                .join(DEFAULT_KEYBINDINGS);
            if packaged.exists() {
                packaged
            } else {
                embedded_root.join(DEFAULT_KEYBINDINGS)
            }
        }
    };
    let mut files = Vec::new();
    let mut seen = Vec::new();
    let mut hotkeys = serde_json::Map::new();
    load_keybinding_chain(&source, &mut files, &mut seen, &mut hotkeys)?;
    let output = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("user-keybindings.json");
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let result =
        serde_json::json!({"controls":{"version":5,"hotkeys":hotkeys,"basePresetPaths":{}}});
    fs::write(
        &output,
        serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    actions::set_user_catalog(&serde_json::to_string(&result).map_err(|e| e.to_string())?)?;
    let rows = hotkeys
        .iter()
        .map(|(action, value)| {
            let binding = value.get("keyboardKey").or_else(|| {
                value
                    .as_array()
                    .and_then(|a| a.iter().find(|entry| entry.get("keyboardKey").is_some()))
                    .and_then(|v| v.get("keyboardKey"))
            });
            let display = binding.and_then(|binding| {
                let codes = match binding {
                    serde_json::Value::Number(number) => {
                        vec![number.as_u64()? as u16]
                    }
                    serde_json::Value::Array(values) => values
                        .iter()
                        .filter_map(serde_json::Value::as_u64)
                        .map(|code| code as u16)
                        .collect::<Vec<_>>(),
                    _ => return None,
                };
                Some(actions::display_key_codes(&codes))
            });
            KeybindingRow {
                action: action.clone(),
                binding: display.unwrap_or_else(|| "—".to_owned()),
            }
        })
        .collect();
    Ok(KeybindingPreview {
        source: source.display().to_string(),
        output: output.display().to_string(),
        files,
        bindings: hotkeys.len(),
        hotkeys: rows,
    })
}

#[cfg(test)]
#[path = "tests/mappings.rs"]
mod tests;
