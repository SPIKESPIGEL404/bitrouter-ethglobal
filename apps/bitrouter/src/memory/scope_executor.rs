//! `NamespaceScopeExecutor` — MCP `Executor` decorator enforcing per-agent
//! Walrus memory namespace scoping (Strategy A).
//!
//! Sits at the innermost layer of the executor stack, below the aggregator and
//! cache, so every call it sees is a resolved `McpTarget::Direct` with the
//! upstream's bare tool name. Non-memory calls and unrestricted agents pass
//! through untouched; scoped agents have `namespace` injected or rejected.

use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::BoxStream;

use bitrouter_sdk::mcp::{Executor, McpRequest, McpResponse, McpStreamPart, McpTarget};
use bitrouter_sdk::{BitrouterError, Result};

use crate::memory::config::{MemoryScopeTable, ScopeDecision};

/// Walrus tools that accept a `namespace` argument and so must be scoped.
const NAMESPACED_TOOLS: &[&str] = &[
    "memwal_remember",
    "memwal_recall",
    "memwal_analyze",
    "memwal_restore",
];

/// Header carrying the calling agent's identity. The orchestrator sets it when
/// spawning a subagent. Absent/empty ⇒ treated as an unknown (restricted) agent.
const AGENT_HEADER: &str = "x-bitrouter-agent";

/// Decorator that enforces namespace scoping over an inner [`Executor`].
pub struct NamespaceScopeExecutor<E: Executor> {
    inner: Arc<E>,
    table: MemoryScopeTable,
}

impl<E: Executor> NamespaceScopeExecutor<E> {
    /// Wrap `inner` with the scope `table`. An empty/disabled table is a pure
    /// passthrough.
    pub fn new(inner: Arc<E>, table: MemoryScopeTable) -> Self {
        Self { inner, table }
    }

    fn agent_of(request: &McpRequest) -> &str {
        request
            .headers
            .get(AGENT_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("")
    }

    fn is_memory_call(&self, target: &McpTarget, request: &McpRequest) -> bool {
        if !self.table.is_enabled() || request.method != "tools/call" {
            return false;
        }
        let on_memory_server = matches!(
            target,
            McpTarget::Direct { server_name, .. } if server_name == self.table.server()
        );
        let tool = request
            .params
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        on_memory_server && NAMESPACED_TOOLS.contains(&tool)
    }

    /// Returns `Ok(Some(rewritten))` when the namespace was injected,
    /// `Ok(None)` to pass through unchanged, or `Err` when the call is denied.
    fn scope(&self, target: &McpTarget, request: &McpRequest) -> Result<Option<McpRequest>> {
        if !self.is_memory_call(target, request) {
            return Ok(None);
        }
        let agent = Self::agent_of(request);
        let requested = request
            .params
            .get("arguments")
            .and_then(|a| a.get("namespace"))
            .and_then(|v| v.as_str());
        match self.table.decide(agent, requested) {
            ScopeDecision::Passthrough => Ok(None),
            ScopeDecision::Inject(ns) => {
                let mut req = request.clone();
                // Ensure `arguments` exists, then set `namespace`.
                if let Some(params) = req.params.as_object_mut() {
                    let args = params
                        .entry("arguments")
                        .or_insert_with(|| serde_json::json!({}));
                    if let Some(obj) = args.as_object_mut() {
                        obj.insert("namespace".to_string(), ns.into());
                    }
                }
                Ok(Some(req))
            }
            ScopeDecision::Deny { agent, requested } => Err(BitrouterError::Unauthorized(format!(
                "agent '{agent}' may not access memory namespace '{requested}'"
            ))),
        }
    }
}

#[async_trait]
impl<E: Executor + 'static> Executor for NamespaceScopeExecutor<E> {
    async fn execute(&self, target: &McpTarget, request: &McpRequest) -> Result<McpResponse> {
        match self.scope(target, request)? {
            Some(rewritten) => self.inner.execute(target, &rewritten).await,
            None => self.inner.execute(target, request).await,
        }
    }

    async fn execute_streaming(
        &self,
        target: &McpTarget,
        request: &McpRequest,
    ) -> Result<BoxStream<'static, Result<McpStreamPart>>> {
        match self.scope(target, request)? {
            Some(rewritten) => self.inner.execute_streaming(target, &rewritten).await,
            None => self.inner.execute_streaming(target, request).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use bitrouter_sdk::caller::CallerContext;
    use bitrouter_sdk::mcp::transport::McpTransport;

    use crate::memory::config::MemoryScopeConfig;

    /// Inner executor that records the request it was handed and returns a
    /// canned ok response.
    struct RecordingExecutor {
        last: Mutex<Option<McpRequest>>,
    }
    impl RecordingExecutor {
        fn new() -> Self {
            Self {
                last: Mutex::new(None),
            }
        }
        fn last_namespace(&self) -> Option<String> {
            self.last
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|r| r.params.get("arguments"))
                .and_then(|a| a.get("namespace"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        }
    }
    #[async_trait]
    impl Executor for RecordingExecutor {
        async fn execute(&self, _t: &McpTarget, request: &McpRequest) -> Result<McpResponse> {
            *self.last.lock().unwrap() = Some(request.clone());
            Ok(McpResponse {
                request_id: request.request_id.clone(),
                result: serde_json::json!({ "ok": true }),
            })
        }
    }

    fn table() -> MemoryScopeTable {
        let cfg: MemoryScopeConfig = serde_json::from_value(serde_json::json!({
            "server": "memory",
            "default_namespace": "shared",
            "agents": {
                "orchestrator": { "namespaces": ["*"] },
                "researcher":   { "namespaces": ["research"], "default": "research" }
            }
        }))
        .unwrap();
        MemoryScopeTable::from_config(&cfg)
    }

    fn memory_target() -> McpTarget {
        McpTarget::Direct {
            server_name: "memory".into(),
            transport: McpTransport::Http {
                url: "https://relayer.example/api/mcp".into(),
                headers: Default::default(),
            },
        }
    }

    fn call(agent: Option<&str>, tool: &str, namespace: Option<&str>) -> McpRequest {
        let mut args = serde_json::Map::new();
        if let Some(ns) = namespace {
            args.insert("namespace".into(), ns.into());
        }
        let params = serde_json::json!({ "name": tool, "arguments": args });
        let mut headers = http::HeaderMap::new();
        if let Some(a) = agent {
            headers.insert("x-bitrouter-agent", a.parse().unwrap());
        }
        McpRequest::direct("memory", "tools/call", params, CallerContext::new("k", "u"))
            .with_headers(headers)
    }

    async fn run(req: McpRequest) -> (Arc<RecordingExecutor>, Result<McpResponse>) {
        let inner = Arc::new(RecordingExecutor::new());
        let exec = NamespaceScopeExecutor::new(inner.clone(), table());
        let res = exec.execute(&memory_target(), &req).await;
        (inner, res)
    }

    #[tokio::test]
    async fn allowed_namespace_passes_through_unchanged() {
        let (inner, res) = run(call(Some("researcher"), "memwal_recall", Some("research"))).await;
        assert!(res.is_ok());
        assert_eq!(inner.last_namespace().as_deref(), Some("research"));
    }

    #[tokio::test]
    async fn disallowed_namespace_is_rejected() {
        let (_inner, res) = run(call(Some("researcher"), "memwal_recall", Some("secret"))).await;
        let err = res.unwrap_err();
        assert_eq!(err.status(), 401);
    }

    #[tokio::test]
    async fn omitted_namespace_gets_agent_default() {
        let (inner, res) = run(call(Some("researcher"), "memwal_remember", None)).await;
        assert!(res.is_ok());
        assert_eq!(inner.last_namespace().as_deref(), Some("research"));
    }

    #[tokio::test]
    async fn unrestricted_agent_is_untouched() {
        let (inner, res) = run(call(Some("orchestrator"), "memwal_recall", Some("anything"))).await;
        assert!(res.is_ok());
        assert_eq!(inner.last_namespace().as_deref(), Some("anything"));
    }

    #[tokio::test]
    async fn unknown_agent_naming_namespace_is_rejected() {
        let (_inner, res) = run(call(None, "memwal_recall", Some("research"))).await;
        assert_eq!(res.unwrap_err().status(), 401);
    }

    #[tokio::test]
    async fn unknown_agent_omitting_namespace_gets_global_default() {
        let (inner, res) = run(call(None, "memwal_recall", None)).await;
        assert!(res.is_ok());
        assert_eq!(inner.last_namespace().as_deref(), Some("shared"));
    }

    #[tokio::test]
    async fn non_namespaced_method_passes_through() {
        // `tools/list` is not a tools/call — never scoped.
        let inner = Arc::new(RecordingExecutor::new());
        let exec = NamespaceScopeExecutor::new(inner.clone(), table());
        let req = McpRequest::direct(
            "memory",
            "tools/list",
            serde_json::json!({}),
            CallerContext::new("k", "u"),
        );
        assert!(exec.execute(&memory_target(), &req).await.is_ok());
    }

    #[tokio::test]
    async fn other_server_passes_through() {
        let inner = Arc::new(RecordingExecutor::new());
        let exec = NamespaceScopeExecutor::new(inner.clone(), table());
        let target = McpTarget::Direct {
            server_name: "not-memory".into(),
            transport: McpTransport::Http {
                url: "https://other.example/mcp".into(),
                headers: Default::default(),
            },
        };
        // researcher naming a forbidden namespace, but not on the memory server.
        let req = call(Some("researcher"), "memwal_recall", Some("secret"));
        assert!(exec.execute(&target, &req).await.is_ok());
    }
}
