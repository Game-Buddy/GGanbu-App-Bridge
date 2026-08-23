// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use std::{collections::HashSet, env, fmt, path::PathBuf};

use http::HeaderValue;
use tauri::{Manager, path::BaseDirectory};
use url::Url;

pub const ALLOWED_ORIGINS_ENV: &str = "GGANBU_BRIDGE_ALLOWED_ORIGINS";
/// Compile-time flag name used to select the release origin policy.
/// Set `GGANBU_RELEASE_BUILD` when compiling a release binary; runtime
/// environment changes cannot switch a binary between release and development
/// origin policies.
pub const RELEASE_BUILD_ENV: &str = "GGANBU_RELEASE_BUILD";
pub const DEFAULT_ORIGINS: [&str; 2] = ["http://localhost:5173", "http://127.0.0.1:5173"];
pub const RELEASE_ORIGINS: [&str; 1] = ["https://gganbu.app"];

/// Returns whether this binary was compiled with the release origin policy.
pub(crate) fn is_release_build() -> bool {
    option_env!("GGANBU_RELEASE_BUILD").is_some()
}

pub fn load_executable_dotenv(app: &tauri::AppHandle) -> Result<Option<PathBuf>, ConfigError> {
    let executable = env::current_exe().map_err(|error| {
        ConfigError::new(format!("could not locate the running executable: {error}"))
    })?;
    let directory = executable.parent().ok_or_else(|| {
        ConfigError::new("the running executable does not have a parent directory")
    })?;
    let bundled = app
        .path()
        .resolve("../.env", BaseDirectory::Resource)
        .map_err(|error| ConfigError::new(format!("could not resolve bundled .env: {error}")))?;
    let mut candidates = vec![bundled];
    let current = env::current_dir().ok();
    for base in [Some(directory), current.as_deref()] {
        let mut ancestor = base;
        for _ in 0..4 {
            let Some(directory) = ancestor else {
                break;
            };
            let path = directory.join(".env");
            if !candidates.contains(&path) {
                candidates.push(path);
            }
            ancestor = directory.parent();
        }
    }

    for path in candidates {
        if !path.try_exists().map_err(|error| {
            ConfigError::new(format!("could not inspect '{}': {error}", path.display()))
        })? {
            continue;
        }

        dotenvy::from_path_override(&path).map_err(|error| {
            ConfigError::new(format!("could not load '{}': {error}", path.display()))
        })?;
        return Ok(Some(path));
    }

    Ok(None)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllowedOrigins {
    values: Vec<String>,
}

impl AllowedOrigins {
    pub fn parse_override(value: Option<&str>) -> Result<Self, ConfigError> {
        match value {
            None => Self::parse_list(DEFAULT_ORIGINS),
            Some(value) if value.trim().is_empty() => Err(ConfigError::new(format!(
                "{ALLOWED_ORIGINS_ENV} is set but empty"
            ))),
            Some(value) => Self::parse_list(value.split(',')),
        }
    }

    pub fn values(&self) -> &[String] {
        &self.values
    }

    pub fn header_values(&self) -> Vec<HeaderValue> {
        self.values
            .iter()
            .map(|origin| {
                HeaderValue::from_str(origin)
                    .expect("validated HTTP origins are valid header values")
            })
            .collect()
    }

    pub fn contains_header(&self, origin: &HeaderValue) -> bool {
        origin
            .to_str()
            .is_ok_and(|value| self.values.iter().any(|allowed| allowed == value))
    }

    fn parse_list<'a>(items: impl IntoIterator<Item = &'a str>) -> Result<Self, ConfigError> {
        let mut seen = HashSet::new();
        let mut values = Vec::new();

        for raw in items {
            let origin = normalize_origin(raw)?;
            if seen.insert(origin.clone()) {
                values.push(origin);
            }
        }

        if values.is_empty() {
            return Err(ConfigError::new("origin allowlist cannot be empty"));
        }

        Ok(Self { values })
    }
}

pub fn read_allowed_origins() -> Result<AllowedOrigins, ConfigError> {
    if is_release_build() {
        return AllowedOrigins::parse_list(RELEASE_ORIGINS);
    }

    match env::var(ALLOWED_ORIGINS_ENV) {
        Ok(value) => AllowedOrigins::parse_override(Some(&value)),
        Err(env::VarError::NotPresent) => AllowedOrigins::parse_override(None),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::new(format!(
            "{ALLOWED_ORIGINS_ENV} contains non-Unicode data"
        ))),
    }
}

fn normalize_origin(raw: &str) -> Result<String, ConfigError> {
    let candidate = raw.trim();
    if candidate.is_empty() {
        return Err(ConfigError::new("origin entries cannot be empty"));
    }
    if candidate.contains('*') {
        return Err(ConfigError::new(format!(
            "wildcards are not allowed in origin '{candidate}'"
        )));
    }

    let parsed = Url::parse(candidate)
        .map_err(|_| ConfigError::new(format!("invalid origin '{candidate}'")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ConfigError::new(format!(
            "origin '{candidate}' must use http:// or https://"
        )));
    }
    if parsed.host_str().is_none() {
        return Err(ConfigError::new(format!(
            "origin '{candidate}' must include a host"
        )));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ConfigError::new(format!(
            "credentials are not allowed in origin '{candidate}'"
        )));
    }
    if parsed.path() != "/" || parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(ConfigError::new(format!(
            "paths, queries, and fragments are not allowed in origin '{candidate}'"
        )));
    }

    let without_trailing_slash = candidate.strip_suffix('/').unwrap_or(candidate);
    let authority = without_trailing_slash
        .split_once("://")
        .map(|(_, authority)| authority)
        .unwrap_or_default();
    if authority.contains('/') || authority.contains('?') || authority.contains('#') {
        return Err(ConfigError::new(format!(
            "paths, queries, and fragments are not allowed in origin '{candidate}'"
        )));
    }

    Ok(parsed.origin().ascii_serialization())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    message: String,
}

impl ConfigError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_local_vite_origins() {
        let origins = AllowedOrigins::parse_override(None).unwrap();
        assert_eq!(origins.values(), DEFAULT_ORIGINS);
    }

    #[test]
    fn release_origins_are_static() {
        assert_eq!(RELEASE_BUILD_ENV, "GGANBU_RELEASE_BUILD");
        assert_eq!(RELEASE_ORIGINS, ["https://gganbu.app"]);
    }

    #[test]
    fn configured_origins_follow_compile_time_build_mode() {
        let origins = read_allowed_origins().unwrap();

        if is_release_build() {
            assert_eq!(origins.values(), RELEASE_ORIGINS);
        } else {
            assert_eq!(origins.values(), DEFAULT_ORIGINS);
        }
    }

    #[test]
    fn override_replaces_defaults_normalizes_and_deduplicates() {
        let origins = AllowedOrigins::parse_override(Some(
            "https://app.example.com/, http://localhost:5173,https://app.example.com",
        ))
        .unwrap();

        assert_eq!(
            origins.values(),
            ["https://app.example.com", "http://localhost:5173"]
        );
    }

    #[test]
    fn invalid_origins_are_rejected() {
        for candidate in [
            "",
            "*",
            "https://*.example.com",
            "ftp://example.com",
            "https://user@example.com",
            "https://example.com/path",
            "https://example.com?query=yes",
            "https://example.com#fragment",
            "not a URL",
            "https://example.com,",
        ] {
            assert!(
                AllowedOrigins::parse_override(Some(candidate)).is_err(),
                "expected '{candidate}' to be rejected"
            );
        }
    }
}
