// this file was originally written by claude
// im sorry but i dont have time to write boilerplate

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Root configuration — maps to the top-level `wrangler.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WranglerConfig {
    pub name: String,

    pub main: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility_date: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<Route>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<Assets>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggers: Option<Triggers>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, EnvConfig>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty", flatten)]
    pub extra: HashMap<String, toml::Value>,
}

impl WranglerConfig {
    /// Gets all binding types from the config.
    ///
    /// # Returns
    /// A vector of `(binding_type, binding_name)`.
    #[inline]
    pub fn get_bindings(&self) -> Vec<(String, String)> {
        self.extra
            .iter()
            .filter_map(|(k, v)| {
                if let Some(value) = v.as_table().and_then(|item| item.get("binding")) {
                    if let Some(binding_name) = value.as_str() {
                        Some((k.clone(), binding_name.to_string()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    }
}

// ── Routes ───────────────────────────────────────────────────────────────────

/// A route can be a bare pattern string or an object with a zone.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Route {
    Pattern(String),
    Object(RouteObject),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteObject {
    pub pattern: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone_name: Option<String>,
}

/// Workers Assets (new, replaces `site` for most use-cases).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assets {
    pub directory: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<String>,
}

// ── Triggers ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Triggers {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crons: Vec<String>,
}

// ── Per-environment overrides ─────────────────────────────────────────────────

/// `[env.<name>]` block — mirrors the top-level fields that can be overridden.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Gets a structured wrangler config from a `&str` slice.
#[inline(always)]
pub fn from_str(s: &str) -> Result<WranglerConfig, toml::de::Error> {
    toml::from_str::<WranglerConfig>(s)
}
