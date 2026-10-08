//! Which providers this server offers (ADR-0028): those in `providers.json`
//! whose key (or, self-hosted, whose URL) is in the environment. Keys stay
//! here: nothing that leaves the server carries one.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The providers file, built in.
const PROVIDERS: &str = include_str!("../providers.json");

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Anthropic,
    Openai,
    Gemini,
}

#[derive(Clone, Debug, Deserialize)]
struct Entry {
    id: String,
    name: String,
    kind: Kind,
    base: Option<String>,
    base_env: Option<String>,
    key_env: Option<String>,
    models_env: Option<String>,
    #[serde(default)]
    models: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Choice {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Deserialize)]
struct File {
    default: Choice,
    providers: Vec<Entry>,
}

/// A provider the server can call.
#[derive(Clone, Debug, PartialEq)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub base: String,
    pub key: Option<String>,
    pub models: Vec<String>,
}

/// The offered providers and the default choice among them.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub providers: Vec<Provider>,
    pub default: Option<Choice>,
}

/// A provider as the browser sees it: no base, no key.
#[derive(Debug, Serialize, PartialEq)]
pub struct PublicModel {
    pub id: String,
    pub default: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PublicProvider {
    pub id: String,
    pub name: String,
    pub models: Vec<PublicModel>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PublicConfig {
    pub providers: Vec<PublicProvider>,
    pub default: Option<Choice>,
}

impl Config {
    /// The providers whose variables `env` has. The default is the file's,
    /// or else the first offered model.
    pub fn from_env(env: &HashMap<String, String>) -> Config {
        let Ok(file) = serde_json::from_str::<File>(PROVIDERS) else {
            return Config {
                providers: Vec::new(),
                default: None,
            };
        };
        let var = |k: &Option<String>| {
            k.as_ref()
                .and_then(|k| env.get(k))
                .filter(|v| !v.trim().is_empty())
                .cloned()
        };
        let providers: Vec<Provider> = file
            .providers
            .into_iter()
            .filter_map(|e| {
                let key = var(&e.key_env);
                if e.key_env.is_some() && key.is_none() {
                    return None;
                }
                let base = var(&e.base_env).or(e.base)?;
                let mut models = e.models;
                if let Some(list) = var(&e.models_env) {
                    models.extend(
                        list.split(',')
                            .map(str::trim)
                            .filter(|m| !m.is_empty())
                            .map(str::to_string),
                    );
                }
                (!models.is_empty()).then_some(Provider {
                    id: e.id,
                    name: e.name,
                    kind: e.kind,
                    base,
                    key,
                    models,
                })
            })
            .collect();
        let offered = |c: &Choice| {
            providers
                .iter()
                .any(|p| p.id == c.provider && p.models.contains(&c.model))
        };
        let default = if offered(&file.default) {
            Some(file.default)
        } else {
            providers.first().and_then(|p| {
                p.models.first().map(|m| Choice {
                    provider: p.id.clone(),
                    model: m.clone(),
                })
            })
        };
        Config { providers, default }
    }

    /// The provider `id` with `model`, when both are offered.
    pub fn find(&self, id: &str, model: &str) -> Option<&Provider> {
        self.providers
            .iter()
            .find(|p| p.id == id && p.models.iter().any(|m| m == model))
    }

    /// What the browser may see.
    pub fn public(&self) -> PublicConfig {
        PublicConfig {
            providers: self
                .providers
                .iter()
                .map(|p| PublicProvider {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    models: p
                        .models
                        .iter()
                        .map(|m| PublicModel {
                            id: m.clone(),
                            default: self
                                .default
                                .as_ref()
                                .is_some_and(|d| d.provider == p.id && &d.model == m),
                        })
                        .collect(),
                })
                .collect(),
            default: self.default.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn only_providers_with_their_variables_are_offered() {
        let none = Config::from_env(&env(&[]));
        assert!(none.providers.is_empty() && none.default.is_none());

        let c = Config::from_env(&env(&[
            ("ANTHROPIC_API_KEY", "sk-a"),
            ("OPENROUTER_API_KEY", " "),
        ]));
        assert_eq!(
            c.providers
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            ["anthropic"],
            "a blank key is no key"
        );
        assert_eq!(
            c.default,
            Some(Choice {
                provider: "anthropic".into(),
                model: "claude-opus-5-5".into()
            })
        );
        assert!(c.find("anthropic", "claude-opus-5-5").is_some());
        assert!(c.find("anthropic", "gpt-9").is_none(), "only the allowlist");

        let local = Config::from_env(&env(&[
            ("ASSIST_SELF_HOSTED_URL", "http://127.0.0.1:11434/v1"),
            ("ASSIST_SELF_HOSTED_MODELS", "qwen3:32b, llama4"),
        ]));
        let p = &local.providers[0];
        assert_eq!(
            (p.id.as_str(), p.base.as_str(), p.key.as_deref()),
            ("self-hosted", "http://127.0.0.1:11434/v1", None)
        );
        assert_eq!(p.models, ["qwen3:32b", "llama4"]);
        assert_eq!(
            local.default,
            Some(Choice {
                provider: "self-hosted".into(),
                model: "qwen3:32b".into()
            }),
            "the first offered"
        );
    }

    #[test]
    fn the_public_config_carries_no_key_or_base() {
        let c = Config::from_env(&env(&[
            ("MISTRAL_API_KEY", "secret-key"),
            ("ANTHROPIC_API_KEY", "sk-ant-secret"),
        ]));
        let json = serde_json::to_string(&c.public()).expect("serialises");
        assert!(
            !json.contains("secret") && !json.contains("https://"),
            "{json}"
        );
        assert!(json.contains("\"default\":true"));
    }
}
