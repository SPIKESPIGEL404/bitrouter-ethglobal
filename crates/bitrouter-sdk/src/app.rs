//! [`App`] and [`AppBuilder`] — the top-level entry point.
//!
//! An [`App`] holds one pipeline per enabled protocol
//! ([`crate::language_model::Pipeline`], [`crate::mcp::Pipeline`],
//! [`crate::acp::Pipeline`]) plus the injected infrastructure (metrics store,
//! metrics renderer, the aggregated migration set).
//!
//! [`AppBuilder`] configures each protocol through its own sub-builder closure
//! ([`language_model`](AppBuilder::language_model), [`mcp`](AppBuilder::mcp),
//! [`acp`](AppBuilder::acp)). A pipeline is built only for protocols that have
//! something configured.
//!
//! # Plugins vs hooks
//!
//! The [`Plugin`] trait is an **optional** convenience: it packages a related
//! set of hooks plus any SQL [`crate::plugin::MigrationItem`]s and installs
//! them through [`AppBuilder::plugin`]. The atomic unit is still a single
//! hook — every plugin can be re-created by calling the relevant
//! sub-builder's hook methods one by one without ever touching [`Plugin`].
//!
//! ```no_run
//! use std::sync::Arc;
//! use bitrouter_sdk::App;
//! use bitrouter_sdk::language_model::{HttpExecutor, StaticRoutingTable};
//!
//! # fn run() -> bitrouter_sdk::Result<()> {
//! let app = App::builder()
//!     .skip_auth(true)
//!     .language_model(|lm| {
//!         lm.routing_table(Arc::new(StaticRoutingTable::new()))
//!           .executor(Arc::new(HttpExecutor::with_defaults().unwrap()));
//!     })
//!     .build()?;
//! # let _ = app; Ok(()) }
//! ```

use std::sync::Arc;

use crate::error::Result;
use crate::language_model::{self, PipelineBuilder};
use crate::metrics::{MetricsRenderer, MetricsStore};
use crate::plugin::{MigrationItem, PluginId};
use crate::{acp, mcp};

/// An optional convenience packaging: registers a related set of hooks +
/// migrations into a builder in one call. `Plugin` is **not** a strong,
/// indivisible unit and **not** the only way to register hooks.
pub trait Plugin {
    /// The plugin's identity (for config mapping and logs).
    fn id(&self) -> &PluginId;

    /// Database migrations carried by this plugin. Empty = no database.
    fn migrations(&self) -> Vec<MigrationItem> {
        Vec::new()
    }

    /// Install this plugin's hooks into the builder.
    fn install(&self, app: &mut AppBuilder);
}

/// A fully assembled application: one pipeline per enabled protocol, plus the
/// injected infrastructure and the collected migration set.
pub struct App {
    language_model: Option<Arc<language_model::Pipeline>>,
    mcp: Option<Arc<mcp::Pipeline>>,
    acp: Option<Arc<acp::Pipeline>>,
    #[allow(dead_code)]
    metrics_store: Option<Arc<dyn MetricsStore>>,
    /// Optional Prometheus-style metrics renderer; if set, the HTTP server
    /// exposes `GET /metrics` against it.
    metrics_renderer: Option<Arc<dyn MetricsRenderer>>,
    migrations: Vec<MigrationItem>,
    skip_auth: bool,
}

impl App {
    /// Start configuring an application.
    pub fn builder() -> AppBuilder {
        AppBuilder::new()
    }

    /// The `language_model` pipeline, if that protocol was configured.
    pub fn language_model(&self) -> Option<&Arc<language_model::Pipeline>> {
        self.language_model.as_ref()
    }

    /// The `mcp` (Model Context Protocol) pipeline, if configured. v1.0 ships
    /// it as pure-routing; the HTTP server mounts `POST /mcp/{name}` against
    /// it.
    pub fn mcp(&self) -> Option<&Arc<mcp::Pipeline>> {
        self.mcp.as_ref()
    }

    /// The `acp` (Agent Client Protocol) pipeline, if configured. v1.0 ships
    /// it as pure-routing; the binary's stdio adapter dispatches against it.
    pub fn acp(&self) -> Option<&Arc<acp::Pipeline>> {
        self.acp.as_ref()
    }

    /// The collected migration set (sorted by version).
    pub fn migrations(&self) -> &[MigrationItem] {
        &self.migrations
    }

    /// Whether `server.skip_auth` is on — when true, credential-less requests
    /// are admitted with a synthesised local caller.
    pub fn skip_auth(&self) -> bool {
        self.skip_auth
    }

    /// The Prometheus-style metrics renderer, if one was wired into the app.
    /// The HTTP server's `GET /metrics` route reads this.
    pub fn metrics_renderer(&self) -> Option<&Arc<dyn MetricsRenderer>> {
        self.metrics_renderer.as_ref()
    }
}

/// Configures an [`App`]. Each protocol is configured through its own
/// sub-builder; `plugin()` is a convenience that drives those sub-builders for
/// you.
pub struct AppBuilder {
    language_model: PipelineBuilder,
    mcp: mcp::PipelineBuilder,
    acp: acp::PipelineBuilder,
    metrics_store: Option<Arc<dyn MetricsStore>>,
    metrics_renderer: Option<Arc<dyn MetricsRenderer>>,
    migrations: Vec<MigrationItem>,
    skip_auth: bool,
}

impl AppBuilder {
    /// A fresh, empty builder.
    pub fn new() -> Self {
        Self {
            language_model: PipelineBuilder::new(),
            mcp: mcp::PipelineBuilder::new(),
            acp: acp::PipelineBuilder::new(),
            metrics_store: None,
            metrics_renderer: None,
            migrations: Vec::new(),
            skip_auth: false,
        }
    }

    /// Set the SDK-level `skip_auth` flag (code default `false`). When `true`,
    /// the server admits credential-less requests with a synthesised local
    /// caller; `AuthHook` still validates any credential that *is* presented.
    pub fn skip_auth(mut self, skip_auth: bool) -> Self {
        self.skip_auth = skip_auth;
        self
    }

    /// Configure the `language_model` protocol pipeline.
    pub fn language_model<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(&mut PipelineBuilder),
    {
        configure(&mut self.language_model);
        self
    }

    /// Configure the `mcp` (Model Context Protocol) protocol pipeline. v1.0
    /// MCP is pure-routing (no settlement); the HTTP server mounts
    /// `POST /mcp/{name}` against the built pipeline.
    pub fn mcp<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(&mut mcp::PipelineBuilder),
    {
        configure(&mut self.mcp);
        self
    }

    /// Configure the `acp` (Agent Client Protocol) protocol pipeline. v1.0
    /// ACP is pure-routing; the binary's stdio adapter dispatches against the
    /// built pipeline.
    pub fn acp<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(&mut acp::PipelineBuilder),
    {
        configure(&mut self.acp);
        self
    }

    /// Inject the `MetricsStore` infrastructure (read by PreRequest hooks,
    /// written by `ReceiptRecorder`).
    pub fn metrics_store(mut self, store: Arc<dyn MetricsStore>) -> Self {
        self.metrics_store = Some(store);
        self
    }

    /// Wire a Prometheus-style metrics renderer. When set, the HTTP server
    /// exposes `GET /metrics` against it. Typically the same
    /// `Arc<PrometheusHook>` you registered as an `ObserveHook`.
    pub fn metrics_renderer(mut self, renderer: Arc<dyn MetricsRenderer>) -> Self {
        self.metrics_renderer = Some(renderer);
        self
    }

    /// Install a `Plugin` convenience package. Equivalent to calling its hook
    /// registrations one by one.
    pub fn plugin(mut self, plugin: impl Plugin) -> Self {
        self.migrations.extend(plugin.migrations());
        plugin.install(&mut self);
        self
    }

    /// Mutable access to the `language_model` sub-builder — the entry point a
    /// `Plugin::install` implementation uses.
    pub fn language_model_builder(&mut self) -> &mut PipelineBuilder {
        &mut self.language_model
    }

    /// Add migrations directly (used by `Plugin::install` when it wants to add
    /// migrations beyond what `Plugin::migrations` declared).
    pub fn add_migrations(&mut self, migrations: impl IntoIterator<Item = MigrationItem>) {
        self.migrations.extend(migrations);
    }

    /// Finalise into an [`App`]. Builds a pipeline for each protocol that was
    /// configured (the `language_model` pipeline needs at least a routing table
    /// and an executor).
    pub fn build(mut self) -> Result<App> {
        let language_model = if self.language_model.is_configured() {
            Some(Arc::new(self.language_model.build()?))
        } else {
            None
        };
        let mcp = if self.mcp.is_configured() {
            Some(Arc::new(self.mcp.build()?))
        } else {
            None
        };
        let acp = if self.acp.is_configured() {
            Some(Arc::new(self.acp.build()?))
        } else {
            None
        };

        self.migrations.sort_by_key(|m| m.version);

        Ok(App {
            language_model,
            mcp,
            acp,
            metrics_store: self.metrics_store,
            metrics_renderer: self.metrics_renderer,
            migrations: self.migrations,
            skip_auth: self.skip_auth,
        })
    }
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}
