// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    collections::HashMap,
    fmt,
    sync::{OnceLock, RwLock},
};

use serde::{Deserialize, Serialize};

const ACTION_MAPPINGS_JSON: &str = include_str!("../action-mappings.json");
static ACTION_CATALOG: OnceLock<ActionCatalog> = OnceLock::new();
static USER_CATALOG: OnceLock<RwLock<Option<ActionCatalog>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetPlatform {
    Linux,
    Windows,
    Macos,
    Unsupported,
}

impl TargetPlatform {
    pub fn current() -> Self {
        if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Unsupported
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ShortcutModifier {
    Control,
    Alt,
    Shift,
    Meta,
}

impl ShortcutModifier {
    fn display_name(self) -> &'static str {
        match self {
            Self::Control => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Meta => "Meta",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedShortcut {
    pub key_codes: Vec<u16>,
    pub modifiers: Vec<ShortcutModifier>,
    pub keys: Vec<String>,
    pub display: String,
}

impl ResolvedShortcut {
    fn from_key_codes(key_codes: &[u16]) -> Self {
        let mut modifiers = Vec::new();
        let mut keys = Vec::new();
        let mut display_parts = Vec::with_capacity(key_codes.len());

        for key_code in key_codes {
            if let Some(modifier) = modifier_for_scan_code(*key_code) {
                if !modifiers.contains(&modifier) {
                    modifiers.push(modifier);
                }
                display_parts.push(modifier.display_name().to_owned());
            } else {
                let key = direct_input_key_name(*key_code);
                display_parts.push(key.clone());
                keys.push(key);
            }
        }

        Self {
            key_codes: key_codes.to_vec(),
            modifiers,
            keys,
            display: display_parts.join(" + "),
        }
    }
}

pub fn display_key_codes(key_codes: &[u16]) -> String {
    ResolvedShortcut::from_key_codes(key_codes).display
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionResolution {
    pub action: String,
    pub label: String,
    pub platform: TargetPlatform,
    pub shortcut: ResolvedShortcut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolveError {
    UnknownAction,
    CatalogUnavailable,
    UnsupportedPlatform,
}

#[derive(Clone, Copy, Debug)]
pub struct ActionResolver {
    platform: TargetPlatform,
    catalog: &'static ActionCatalog,
}

impl ActionResolver {
    pub fn for_current_platform() -> Self {
        Self::for_platform(TargetPlatform::current())
    }

    pub fn for_platform(platform: TargetPlatform) -> Self {
        Self {
            platform,
            catalog: embedded_catalog(),
        }
    }

    pub fn platform(&self) -> TargetPlatform {
        self.platform
    }

    pub fn resolve(&self, action: &str) -> Result<ActionResolution, ResolveError> {
        if self.platform == TargetPlatform::Unsupported {
            return Err(ResolveError::UnsupportedPlatform);
        }

        let user_catalog = USER_CATALOG
            .get()
            .map(|catalog| catalog.read().map_err(|_| ResolveError::CatalogUnavailable))
            .transpose()?;
        let catalog = user_catalog
            .as_ref()
            .and_then(|catalog| catalog.as_ref())
            .unwrap_or(self.catalog);
        let key_codes = catalog
            .mappings
            .get(action)
            .ok_or(ResolveError::UnknownAction)?;

        Ok(ActionResolution {
            action: action.to_owned(),
            label: action_label(action),
            platform: self.platform,
            shortcut: ResolvedShortcut::from_key_codes(key_codes),
        })
    }
}

#[derive(Debug, Deserialize)]
struct MappingDocument {
    controls: Controls,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Controls {
    version: u16,
    hotkeys: HashMap<String, HotkeyEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum HotkeyEntry {
    Binding(InputBinding),
    Alternatives(Vec<InputBinding>),
}

impl HotkeyEntry {
    fn first_keyboard_binding(&self) -> Option<Vec<u16>> {
        match self {
            Self::Binding(binding) => binding.keyboard_key.as_ref().map(KeyCodes::to_vec),
            Self::Alternatives(bindings) => bindings
                .iter()
                .find_map(|binding| binding.keyboard_key.as_ref().map(KeyCodes::to_vec)),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InputBinding {
    keyboard_key: Option<KeyCodes>,
    #[serde(flatten)]
    _other_input: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum KeyCodes {
    One(u16),
    Chord(Vec<u16>),
}

impl KeyCodes {
    fn to_vec(&self) -> Vec<u16> {
        match self {
            Self::One(key_code) => vec![*key_code],
            Self::Chord(key_codes) => key_codes.clone(),
        }
    }
}

#[derive(Debug)]
struct ActionCatalog {
    mappings: HashMap<String, Vec<u16>>,
}

impl ActionCatalog {
    fn parse(json: &str) -> Result<Self, MappingError> {
        let document: MappingDocument = serde_json::from_str(json)
            .map_err(|error| MappingError::new(format!("invalid JSON: {error}")))?;

        if document.controls.version == 0 {
            return Err(MappingError::new(
                "controls.version must be greater than zero",
            ));
        }
        if document.controls.hotkeys.is_empty() {
            return Err(MappingError::new("controls.hotkeys must not be empty"));
        }

        let mut mappings = HashMap::new();
        for (action, entry) in document.controls.hotkeys {
            if !is_valid_action_code(&action) {
                return Err(MappingError::new(format!("invalid action code `{action}`")));
            }

            let Some(key_codes) = entry.first_keyboard_binding() else {
                continue;
            };
            validate_key_codes(&action, &key_codes)?;
            mappings.insert(action, key_codes);
        }

        if mappings.is_empty() {
            return Err(MappingError::new(
                "controls.hotkeys must contain at least one keyboard binding",
            ));
        }

        Ok(Self { mappings })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct MappingError {
    message: String,
}

impl MappingError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MappingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for MappingError {}

pub fn set_user_catalog(json: &str) -> Result<(), String> {
    let catalog = ActionCatalog::parse(json).map_err(|error| error.to_string())?;
    let storage = USER_CATALOG.get_or_init(|| RwLock::new(None));
    *storage
        .write()
        .map_err(|_| "mapping catalog lock poisoned".to_owned())? = Some(catalog);
    Ok(())
}

fn embedded_catalog() -> &'static ActionCatalog {
    ACTION_CATALOG.get_or_init(|| {
        ActionCatalog::parse(ACTION_MAPPINGS_JSON)
            .expect("embedded action-mappings.json must contain valid War Thunder hotkeys")
    })
}

fn validate_key_codes(action: &str, key_codes: &[u16]) -> Result<(), MappingError> {
    if key_codes.is_empty() || key_codes.len() > 8 {
        return Err(MappingError::new(format!(
            "action `{action}` must contain between 1 and 8 keyboard scan codes"
        )));
    }
    if key_codes.contains(&0) {
        return Err(MappingError::new(format!(
            "action `{action}` contains invalid keyboard scan code 0"
        )));
    }
    if key_codes
        .iter()
        .enumerate()
        .any(|(index, key_code)| key_codes[..index].contains(key_code))
    {
        return Err(MappingError::new(format!(
            "action `{action}` contains duplicate keyboard scan codes"
        )));
    }

    Ok(())
}

pub fn is_valid_action_code(action: &str) -> bool {
    let mut characters = action.chars();
    let Some(first) = characters.next() else {
        return false;
    };

    action.len() <= 128
        && first.is_ascii_alphabetic()
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn action_label(action: &str) -> String {
    action
        .strip_prefix("ID_")
        .unwrap_or(action)
        .split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            if matches!(
                word,
                "AI" | "AAM"
                    | "AGM"
                    | "ATGM"
                    | "FPS"
                    | "HUD"
                    | "IR"
                    | "IRST"
                    | "MFD"
                    | "NVG"
                    | "RWR"
                    | "TV"
                    | "UAV"
            ) {
                word.to_ascii_uppercase()
            } else {
                let mut characters = word.chars();
                let first = characters
                    .next()
                    .map(|character| character.to_ascii_uppercase())
                    .unwrap_or_default();
                format!("{first}{}", characters.as_str().to_ascii_lowercase())
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn modifier_for_scan_code(scan_code: u16) -> Option<ShortcutModifier> {
    match scan_code {
        29 | 157 => Some(ShortcutModifier::Control),
        42 | 54 => Some(ShortcutModifier::Shift),
        56 | 184 => Some(ShortcutModifier::Alt),
        219 | 220 => Some(ShortcutModifier::Meta),
        _ => None,
    }
}

fn direct_input_key_name(scan_code: u16) -> String {
    let name = match scan_code {
        1 => "Escape",
        2 => "1",
        3 => "2",
        4 => "3",
        5 => "4",
        6 => "5",
        7 => "6",
        8 => "7",
        9 => "8",
        10 => "9",
        11 => "0",
        12 => "-",
        13 => "=",
        14 => "Backspace",
        15 => "Tab",
        16 => "Q",
        17 => "W",
        18 => "E",
        19 => "R",
        20 => "T",
        21 => "Y",
        22 => "U",
        23 => "I",
        24 => "O",
        25 => "P",
        26 => "[",
        27 => "]",
        28 => "Enter",
        30 => "A",
        31 => "S",
        32 => "D",
        33 => "F",
        34 => "G",
        35 => "H",
        36 => "J",
        37 => "K",
        38 => "L",
        39 => ";",
        40 => "'",
        41 => "`",
        43 => "\\",
        44 => "Z",
        45 => "X",
        46 => "C",
        47 => "V",
        48 => "B",
        49 => "N",
        50 => "M",
        51 => ",",
        52 => ".",
        53 => "/",
        55 => "Numpad *",
        57 => "Space",
        58 => "Caps Lock",
        59 => "F1",
        60 => "F2",
        61 => "F3",
        62 => "F4",
        63 => "F5",
        64 => "F6",
        65 => "F7",
        66 => "F8",
        67 => "F9",
        68 => "F10",
        69 => "Num Lock",
        70 => "Scroll Lock",
        71 => "Numpad 7",
        72 => "Numpad 8",
        73 => "Numpad 9",
        74 => "Numpad -",
        75 => "Numpad 4",
        76 => "Numpad 5",
        77 => "Numpad 6",
        78 => "Numpad +",
        79 => "Numpad 1",
        80 => "Numpad 2",
        81 => "Numpad 3",
        82 => "Numpad 0",
        83 => "Numpad .",
        87 => "F11",
        88 => "F12",
        100 => "F13",
        101 => "F14",
        102 => "F15",
        141 => "Numpad =",
        144 => "Previous Track",
        153 => "Next Track",
        156 => "Numpad Enter",
        160 => "Mute",
        161 => "Calculator",
        162 => "Play/Pause",
        164 => "Media Stop",
        174 => "Volume Down",
        176 => "Volume Up",
        178 => "Web Home",
        179 => "Numpad ,",
        181 => "Numpad /",
        183 => "Print Screen",
        197 => "Pause",
        199 => "Home",
        200 => "Arrow Up",
        201 => "Page Up",
        203 => "Arrow Left",
        205 => "Arrow Right",
        207 => "End",
        208 => "Arrow Down",
        209 => "Page Down",
        210 => "Insert",
        211 => "Delete",
        221 => "Menu",
        222 => "Power",
        223 => "Sleep",
        227 => "Wake",
        229 => "Web Search",
        230 => "Web Favorites",
        231 => "Web Refresh",
        232 => "Web Stop",
        233 => "Web Forward",
        234 => "Web Back",
        235 => "My Computer",
        236 => "Mail",
        237 => "Media Select",
        _ => return format!("Scan code {scan_code}"),
    };

    name.to_owned()
}

#[cfg(test)]
#[path = "tests/actions.rs"]
mod tests;
