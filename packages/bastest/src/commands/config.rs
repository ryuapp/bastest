use jsonc_parser::parse_to_serde_value;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BastestConfig {
    pub agent: Option<bool>,
    pub concurrency: Option<usize>,
    pub exclude: Option<Vec<String>>,
    pub fail_fast: Option<bool>,
    pub filter: Option<String>,
    pub in_source_test: Option<bool>,
    pub typecheck: Option<TypecheckConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypecheckConfig {
    pub enabled: Option<bool>,
    pub checker: Option<TypecheckChecker>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TypecheckChecker {
    Tsc,
    Tsgo,
}

impl TypecheckChecker {
    pub fn bin_name(self) -> &'static str {
        match self {
            Self::Tsc => "tsc",
            Self::Tsgo => "tsgo",
        }
    }
}

pub fn load(cwd: &Path) -> Result<BastestConfig, i32> {
    let config_path =
        if let Some(config_path) = std::env::var_os("BASTEST_CONFIG_PATH").map(PathBuf::from) {
            config_path
        } else {
            let config_path = cwd.join("bastest.jsonc");
            if !config_path.is_file() {
                return Ok(BastestConfig::default());
            }
            config_path
        };

    let config = std::fs::read_to_string(&config_path).map_err(|error| {
        if std::env::var_os("BASTEST_CONFIG_PATH").is_some() {
            eprintln!(
                "failed to read BASTEST_CONFIG_PATH {}: {error}",
                config_path.display()
            );
        } else {
            eprintln!("failed to read {}: {error}", config_path.display());
        }
        1
    })?;
    parse_to_serde_value::<BastestConfig>(&config, &Default::default()).map_err(|error| {
        if std::env::var_os("BASTEST_CONFIG_PATH").is_some() {
            eprintln!(
                "failed to parse BASTEST_CONFIG_PATH {}: {error}",
                config_path.display()
            );
        } else {
            eprintln!("failed to parse {}: {error}", config_path.display());
        }
        1
    })
}

pub fn in_source_test_enabled(config: &BastestConfig) -> bool {
    config.in_source_test.unwrap_or(true)
}

pub fn exclude(config: &BastestConfig) -> Option<&[String]> {
    config.exclude.as_deref()
}

pub fn agent_enabled(config: &BastestConfig) -> bool {
    config.agent.unwrap_or(false) || agent_env_enabled()
}

fn agent_env_enabled() -> bool {
    std::env::var_os("AI_AGENT").is_some()
        || std::env::var_os("CODEX_THREAD_ID").is_some()
        || std::env::var_os("CLAUDECODE").is_some()
}

pub fn typecheck_enabled(config: &BastestConfig) -> bool {
    config
        .typecheck
        .as_ref()
        .and_then(|typecheck| typecheck.enabled)
        .unwrap_or(false)
}

pub fn typecheck_checker(config: &BastestConfig) -> TypecheckChecker {
    config
        .typecheck
        .as_ref()
        .and_then(|typecheck| typecheck.checker)
        .unwrap_or(TypecheckChecker::Tsc)
}
