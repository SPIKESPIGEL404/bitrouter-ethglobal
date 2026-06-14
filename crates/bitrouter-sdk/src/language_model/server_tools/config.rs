//! Configuration for the server-side tool loop: the runtime
//! [`ServerToolLoopConfig`] bounds and the deserialised [`ServerToolsConfig`]
//! YAML section.

use std::time::Duration;

use serde::Deserialize;

/// Bounds the [`ServerToolLoop`](super::loop_controller::ServerToolLoop): how
/// many tool rounds it runs, how long each tool may take, the total wall-clock
/// budget for the turn, and how many consecutive tool-error rounds it tolerates
/// before giving up.
#[derive(Debug, Clone)]
pub struct ServerToolLoopConfig {
    /// Maximum number of tool-execution rounds. Reaching it terminates the
    /// loop with a truncation finish reason. Default 10.
    pub max_iterations: u32,
    /// Per-tool execution timeout. Default 30s.
    pub tool_timeout: Duration,
    /// Total wall-clock budget for the whole turn. Default 120s.
    pub total_budget: Duration,
    /// Consecutive tool-error rounds tolerated before giving up. Default 3.
    pub max_consecutive_errors: u32,
}

impl Default for ServerToolLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            tool_timeout: Duration::from_secs(30),
            total_budget: Duration::from_secs(120),
            max_consecutive_errors: 3,
        }
    }
}

/// The OSS `server_tools` config section. Names the MCP servers whose tools
/// BitRouter attaches to LLM requests and executes inside the loop, with an
/// optional override of the loop's iteration cap. An empty `mcp_servers`
/// leaves the pipeline strictly single-shot.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ServerToolsConfig {
    /// MCP server names (keys of `mcp_servers`) whose tools are injected into
    /// LLM requests and executed by the loop.
    pub mcp_servers: Vec<String>,
    /// Optional override of the loop's maximum tool-execution rounds.
    pub max_iterations: Option<u32>,
    /// When set, enables the router-owned `spawn_subagent` tool (in addition to
    /// any `mcp_servers`). The agent calls it to spawn a budgeted subagent.
    pub spawn_subagent: Option<SpawnSubagentConfig>,
}

/// One entry of the `spawn_subagent` model catalog: either a bare model id, or
/// an object carrying capability tags + an indicative price so the orchestrator
/// can choose a model by cost / capability (PRD §8.3). Deserialises from a plain
/// string *or* a `{ id, caps, price }` object, so a flat allowlist of ids keeps
/// working unchanged.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ModelChoice {
    /// A bare model id, no catalog metadata.
    Id(String),
    /// A catalog entry: id plus optional capability tags and indicative price.
    Entry {
        /// The model id (matched against the `spawn_subagent` call's `model`).
        id: String,
        /// Capability tags (e.g. `["text", "long-context"]`); free-form, used
        /// only to inform the orchestrator's choice.
        #[serde(default)]
        caps: Vec<String>,
        /// Indicative price (e.g. dollars per 1k output tokens), for display.
        #[serde(default)]
        price: Option<String>,
    },
}

impl ModelChoice {
    /// The model id, regardless of representation.
    pub fn id(&self) -> &str {
        match self {
            Self::Id(id) => id.as_str(),
            Self::Entry { id, .. } => id.as_str(),
        }
    }

    /// Capability tags — empty for a bare id.
    pub fn caps(&self) -> &[String] {
        match self {
            Self::Id(_) => &[],
            Self::Entry { caps, .. } => caps,
        }
    }

    /// Indicative price, if catalogued.
    pub fn price(&self) -> Option<&str> {
        match self {
            Self::Id(_) => None,
            Self::Entry { price, .. } => price.as_deref(),
        }
    }
}

impl From<&str> for ModelChoice {
    fn from(s: &str) -> Self {
        Self::Id(s.to_string())
    }
}

impl From<String> for ModelChoice {
    fn from(s: String) -> Self {
        Self::Id(s)
    }
}

/// Settings for the `spawn_subagent` router tool. Names the model catalog a
/// spawned worker may use, the base URL the worker should call back on (the
/// local daemon, so the worker's inferences are metered), and the worker
/// command.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SpawnSubagentConfig {
    /// The daemon URL the spawned worker routes its inferences to. Must point at
    /// THIS daemon (e.g. `http://127.0.0.1:4356/v1`) so the worker's calls carry
    /// its scoped `brvk_` and are metered + capped here.
    pub base_url: String,
    /// The worker command to spawn in ACP mode. Default `"opencode"`.
    pub command: String,
    /// The catalog of models a spawned worker may use, each a bare id or a
    /// `{ id, caps, price }` entry. A `spawn_subagent` call naming a model
    /// outside this catalog is rejected.
    pub models: Vec<ModelChoice>,
}

impl Default for SpawnSubagentConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:4356/v1".to_string(),
            command: "opencode".to_string(),
            models: Vec::new(),
        }
    }
}

#[cfg(all(test, feature = "config_file"))]
mod spawn_subagent_config_tests {
    use super::*;

    #[test]
    fn deserializes_spawn_subagent_section() {
        let yaml = r#"
mcp_servers: []
spawn_subagent:
  base_url: "http://127.0.0.1:4356/v1"
  command: "opencode"
  models:
    - "bitrouter/z-ai/glm-5.1"
"#;
        let cfg: ServerToolsConfig = serde_saphyr::from_str(yaml).unwrap();
        let sa = cfg.spawn_subagent.expect("section present");
        assert_eq!(sa.base_url, "http://127.0.0.1:4356/v1");
        assert_eq!(sa.command, "opencode");
        let ids: Vec<&str> = sa.models.iter().map(|m| m.id()).collect();
        assert_eq!(ids, vec!["bitrouter/z-ai/glm-5.1"]);
    }

    #[test]
    fn deserializes_model_catalog_entries() {
        // The catalog accepts bare ids and `{ id, caps, price }` entries side by
        // side, so an orchestrator can choose by cost / capability (PRD §8.3).
        let yaml = r#"
mcp_servers: []
spawn_subagent:
  models:
    - "bitrouter/cheap"
    - id: "bitrouter/smart"
      caps: ["text", "long-context"]
      price: "0.006"
"#;
        let cfg: ServerToolsConfig = serde_saphyr::from_str(yaml).unwrap();
        let sa = cfg.spawn_subagent.expect("section present");
        assert_eq!(sa.models[0].id(), "bitrouter/cheap");
        assert!(sa.models[0].caps().is_empty());
        assert_eq!(sa.models[0].price(), None);
        let smart = &sa.models[1];
        assert_eq!(smart.id(), "bitrouter/smart");
        assert_eq!(smart.caps(), ["text", "long-context"]);
        assert_eq!(smart.price(), Some("0.006"));
    }

    #[test]
    fn spawn_subagent_absent_by_default() {
        let cfg: ServerToolsConfig = serde_saphyr::from_str("mcp_servers: []").unwrap();
        assert!(cfg.spawn_subagent.is_none());
    }
}
