use std::env;

use thiserror::Error;

use crate::agents::AgentId;
use crate::credentials::{AuthType, ExtractedCredentials, ProviderCredentials};

#[derive(Debug, Clone)]
pub struct TestAgentConfig {
    pub agent: AgentId,
    pub credentials: ExtractedCredentials,
}

#[derive(Debug, Error)]
pub enum TestAgentConfigError {
    #[error("no test agents configured (set SANDBOX_TEST_AGENTS)")]
    NoAgentsConfigured,
    #[error("unknown agent name: {0}")]
    UnknownAgent(String),
    #[error("missing credentials for {agent}: {missing}")]
    MissingCredentials { agent: AgentId, missing: String },
}

const AGENTS_ENV: &str = "SANDBOX_TEST_AGENTS";
const ANTHROPIC_ENV: &str = "SANDBOX_TEST_ANTHROPIC_API_KEY";
const OPENAI_ENV: &str = "SANDBOX_TEST_OPENAI_API_KEY";

pub fn test_agents_from_env() -> Result<Vec<TestAgentConfig>, TestAgentConfigError> {
    let raw_agents = env::var(AGENTS_ENV).unwrap_or_default();
    let mut agents = Vec::new();
    for entry in raw_agents.split(',') {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "all" {
            agents.extend([
                AgentId::Claude,
                AgentId::Codex,
                AgentId::Opencode,
                AgentId::Amp,
            ]);
            continue;
        }
        let agent = AgentId::parse(trimmed)
            .ok_or_else(|| TestAgentConfigError::UnknownAgent(trimmed.to_string()))?;
        agents.push(agent);
    }

    if agents.is_empty() {
        return Err(TestAgentConfigError::NoAgentsConfigured);
    }

    let anthropic_key = read_env_key(ANTHROPIC_ENV);
    let openai_key = read_env_key(OPENAI_ENV);

    let mut configs = Vec::new();
    for agent in agents {
        let credentials = match agent {
            AgentId::Claude | AgentId::Amp => {
                let anthropic_key = anthropic_key.clone().ok_or_else(|| {
                    TestAgentConfigError::MissingCredentials {
                        agent,
                        missing: ANTHROPIC_ENV.to_string(),
                    }
                })?;
                credentials_with(anthropic_key, None)
            }
            AgentId::Codex => {
                let openai_key = openai_key.clone().ok_or_else(|| {
                    TestAgentConfigError::MissingCredentials {
                        agent,
                        missing: OPENAI_ENV.to_string(),
                    }
                })?;
                credentials_with(None, Some(openai_key))
            }
            AgentId::Opencode => {
                if anthropic_key.is_none() && openai_key.is_none() {
                    return Err(TestAgentConfigError::MissingCredentials {
                        agent,
                        missing: format!("{ANTHROPIC_ENV} or {OPENAI_ENV}"),
                    });
                }
                credentials_with(anthropic_key.clone(), openai_key.clone())
            }
        };
        configs.push(TestAgentConfig { agent, credentials });
    }

    Ok(configs)
}

fn read_env_key(name: &str) -> Option<String> {
    env::var(name).ok().and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn credentials_with(
    anthropic_key: Option<String>,
    openai_key: Option<String>,
) -> ExtractedCredentials {
    let mut credentials = ExtractedCredentials::default();
    if let Some(key) = anthropic_key {
        credentials.anthropic = Some(ProviderCredentials {
            api_key: key,
            source: "sandbox-test-env".to_string(),
            auth_type: AuthType::ApiKey,
            provider: "anthropic".to_string(),
        });
    }
    if let Some(key) = openai_key {
        credentials.openai = Some(ProviderCredentials {
            api_key: key,
            source: "sandbox-test-env".to_string(),
            auth_type: AuthType::ApiKey,
            provider: "openai".to_string(),
        });
    }
    credentials
}
