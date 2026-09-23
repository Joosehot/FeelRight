//! Rule weights and parameters, loaded from `rules.toml`.

use anyhow::{anyhow, Context as _, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// Built-in defaults, used when no `rules.toml` is found.
pub const DEFAULT_TOML: &str = include_str!("../rules.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct RuleConfig {
    pub weight: f32,
    #[serde(default)]
    pub breakable: bool,
    /// Tension added to a bar when this rule is broken there.
    #[serde(default)]
    pub tension: f32,
    /// Rule-specific numeric parameters.
    #[serde(flatten)]
    pub params: HashMap<String, f32>,
}

impl RuleConfig {
    /// Fetch a required parameter. Missing parameters are a config error,
    /// reported at load time by `Config::validate`.
    pub fn param(&self, name: &str) -> f32 {
        *self
            .params
            .get(name)
            .unwrap_or_else(|| panic!("rules.toml: missing parameter '{name}' (validated at load)"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchConfig {
    /// Beam width (M3).
    #[serde(default = "default_beam")]
    pub beam: usize,
    /// Weight of the tension-match term (M5).
    #[serde(default)]
    pub tension_lambda: f32,
    /// Pitch candidates sampled per beam entry per bar.
    #[serde(default = "default_candidates")]
    pub candidates_per_bar: usize,
    /// Largest interval (semitones) the candidate sampler will propose.
    #[serde(default = "default_max_interval")]
    pub max_interval: i32,
    /// How much of a breakable rule's penalty is forgiven at target
    /// tension 1.0 (0 = never forgive, 1 = fully forgive).
    #[serde(default)]
    pub breakable_discount: f32,
}

/// Weights for the observed-tension estimate.
#[derive(Debug, Clone, Deserialize)]
pub struct TensionConfig {
    #[serde(default = "one")]
    pub broken_rules: f32,
    #[serde(default = "one")]
    pub dissonance: f32,
    #[serde(default = "one")]
    pub register: f32,
    #[serde(default = "one")]
    pub leap: f32,
    #[serde(default = "one")]
    pub density: f32,
    #[serde(default = "one")]
    pub syncopation: f32,
    #[serde(default = "one")]
    pub instability: f32,
    /// Sum of components is divided by this before clamping to 0..1.
    #[serde(default = "default_scale")]
    pub scale: f32,
}

fn one() -> f32 {
    1.0
}
fn default_scale() -> f32 {
    3.0
}
fn default_tension() -> TensionConfig {
    TensionConfig {
        broken_rules: 1.0,
        dissonance: 1.0,
        register: 1.0,
        leap: 1.0,
        density: 1.0,
        syncopation: 1.0,
        instability: 1.0,
        scale: default_scale(),
    }
}

fn default_beam() -> usize {
    64
}
fn default_candidates() -> usize {
    32
}
fn default_max_interval() -> i32 {
    9
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_search")]
    pub search: SearchConfig,
    #[serde(default = "default_tension")]
    pub tension: TensionConfig,
    pub rules: HashMap<String, RuleConfig>,
}

fn default_search() -> SearchConfig {
    SearchConfig {
        beam: default_beam(),
        tension_lambda: 0.0,
        candidates_per_bar: default_candidates(),
        max_interval: default_max_interval(),
        breakable_discount: 0.0,
    }
}

impl Config {
    pub fn from_str(text: &str) -> Result<Self> {
        toml::from_str(text).context("parsing rules config")
    }

    pub fn default_config() -> Self {
        Self::from_str(DEFAULT_TOML).expect("embedded rules.toml is valid")
    }

    /// Load from `path` if given, else `./rules.toml` if present, else defaults.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let path = match path {
            Some(p) => p.to_path_buf(),
            None => {
                let p = Path::new("rules.toml");
                if !p.exists() {
                    return Ok(Self::default_config());
                }
                p.to_path_buf()
            }
        };
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        Self::from_str(&text)
    }

    pub fn rule(&self, name: &str) -> Result<&RuleConfig> {
        self.rules
            .get(name)
            .ok_or_else(|| anyhow!("rules.toml has no [rules.{name}] section"))
    }

    /// Check that every registered rule has a section and all its
    /// required parameters.
    pub fn validate(&self, rules: &[Box<dyn crate::rules::Rule>]) -> Result<()> {
        for r in rules {
            let cfg = self.rule(r.name())?;
            for p in r.params() {
                if !cfg.params.contains_key(*p) {
                    return Err(anyhow!("rules.toml: [rules.{}] is missing '{p}'", r.name()));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_defaults_cover_all_rules() {
        let cfg = Config::default_config();
        cfg.validate(&crate::rules::all_rules()).unwrap();
        assert!(cfg.rule("chord_tones_on_strong_beats").unwrap().weight > 0.0);
    }

    #[test]
    fn missing_param_is_an_error() {
        let cfg = Config::from_str(
            "[rules.mostly_steps]\nweight = 1.0\n",
        )
        .unwrap();
        assert!(cfg.validate(&crate::rules::all_rules()).is_err());
    }
}
