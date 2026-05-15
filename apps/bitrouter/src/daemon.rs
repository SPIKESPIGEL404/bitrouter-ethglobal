//! Daemon control over a Unix domain socket (007 §6.1).
//!
//! A running `bitrouter serve` listens on a control socket alongside the HTTP
//! API. The CLI's `stop` / `restart` / `reload` / `status` / `route`
//! subcommands are thin clients that connect, send one newline-delimited JSON
//! [`DaemonCommand`], and read one [`DaemonResponse`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use bitrouter_sdk::App;
use bitrouter_sdk::caller::CallerContext;
use bitrouter_sdk::language_model::RoutingPrefs;

/// A command sent from the CLI to a running daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum DaemonCommand {
    /// Stop the daemon — it finishes the response, then exits.
    Stop,
    /// Hot-reload the config / routing table.
    Reload,
    /// Report daemon status.
    Status,
    /// Resolve a model name through the live routing table.
    Route {
        /// The model name to resolve.
        model: String,
    },
}

/// One resolved hop of a route chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteHop {
    /// Provider id.
    pub provider: String,
    /// Service / model id at the provider.
    pub service_id: String,
    /// The wire protocol for the hop.
    pub api_protocol: String,
}

/// The daemon's reply to a [`DaemonCommand`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "resp", rename_all = "snake_case")]
pub enum DaemonResponse {
    /// The command succeeded with no payload.
    Ok,
    /// Status payload.
    Status {
        /// The daemon's process id.
        pid: u32,
        /// The HTTP listen address.
        listen: String,
        /// Count of routable models.
        models: usize,
    },
    /// A resolved route chain.
    Route {
        /// The ordered fallback chain.
        chain: Vec<RouteHop>,
    },
    /// The command failed.
    Error {
        /// Human-readable failure detail.
        message: String,
    },
}

/// The default control-socket path when the config does not set one.
pub const DEFAULT_CONTROL_SOCKET: &str = "./bitrouter.sock";

// ===== server side =====

/// Run the control-socket listener until a `Stop` command is received (then it
/// returns) — run this alongside `App::serve` under a `tokio::select!`.
///
/// `listen` is the HTTP address (reported in `Status`); the socket is bound at
/// `socket_path` and removed on return.
pub async fn run_control_socket(socket_path: PathBuf, app: Arc<App>, listen: String) -> Result<()> {
    // A stale socket file from a crashed daemon would block the bind.
    let _ = tokio::fs::remove_file(&socket_path).await;
    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("binding control socket {}", socket_path.display()))?;
    // The control surface includes Stop / Reload — only the daemon owner may
    // reach it. UnixListener::bind respects the process umask (typically 022 →
    // 0755); tighten to 0600 explicitly so any other local user is excluded.
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))
        .await
        .with_context(|| format!("chmod 0600 {}", socket_path.display()))?;
    tracing::info!(socket = %socket_path.display(), "control socket listening (mode 0600)");

    let result = accept_loop(&listener, &app, &listen).await;
    let _ = tokio::fs::remove_file(&socket_path).await;
    result
}

async fn accept_loop(listener: &UnixListener, app: &Arc<App>, listen: &str) -> Result<()> {
    loop {
        let (stream, _addr) = listener
            .accept()
            .await
            .context("accepting control-socket connection")?;
        // Handle one command per connection. A `Stop` ends the loop (and thus
        // the whole `serve`); any other command loops for the next client.
        if handle_connection(stream, app, listen).await? {
            tracing::info!("stop command received — shutting down");
            return Ok(());
        }
    }
}

/// Handle one connection. Returns `Ok(true)` if it was a `Stop` command.
async fn handle_connection(stream: UnixStream, app: &Arc<App>, listen: &str) -> Result<bool> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).await? == 0 {
        return Ok(false); // client hung up without sending anything
    }
    let command: DaemonCommand = match serde_json::from_str(line.trim()) {
        Ok(c) => c,
        Err(e) => {
            write_response(
                reader.get_mut(),
                &DaemonResponse::Error {
                    message: format!("invalid command: {e}"),
                },
            )
            .await?;
            return Ok(false);
        }
    };

    let is_stop = matches!(command, DaemonCommand::Stop);
    let response = dispatch(command, app, listen).await;
    write_response(reader.get_mut(), &response).await?;
    Ok(is_stop)
}

async fn dispatch(command: DaemonCommand, app: &Arc<App>, listen: &str) -> DaemonResponse {
    match command {
        DaemonCommand::Stop => DaemonResponse::Ok,
        DaemonCommand::Reload => match app.language_model() {
            Some(pipeline) => match pipeline.routing_table().reload().await {
                Ok(()) => {
                    tracing::info!("config reloaded");
                    DaemonResponse::Ok
                }
                Err(e) => DaemonResponse::Error {
                    message: format!("reload failed: {e}"),
                },
            },
            None => DaemonResponse::Error {
                message: "no language_model pipeline configured".to_string(),
            },
        },
        DaemonCommand::Status => {
            let models = app
                .language_model()
                .map(|p| p.routing_table().list_models().len())
                .unwrap_or(0);
            DaemonResponse::Status {
                pid: std::process::id(),
                listen: listen.to_string(),
                models,
            }
        }
        DaemonCommand::Route { model } => {
            let Some(pipeline) = app.language_model() else {
                return DaemonResponse::Error {
                    message: "no language_model pipeline configured".to_string(),
                };
            };
            match pipeline
                .routing_table()
                .route_chain(&model, &RoutingPrefs::default(), &CallerContext::local())
                .await
            {
                Ok(chain) => DaemonResponse::Route {
                    chain: chain
                        .into_iter()
                        .map(|t| RouteHop {
                            provider: t.provider_name,
                            service_id: t.service_id,
                            api_protocol: format!("{:?}", t.api_protocol).to_lowercase(),
                        })
                        .collect(),
                },
                Err(e) => DaemonResponse::Error {
                    message: e.to_string(),
                },
            }
        }
    }
}

async fn write_response(stream: &mut UnixStream, response: &DaemonResponse) -> Result<()> {
    let mut json = serde_json::to_string(response).context("serialising daemon response")?;
    json.push('\n');
    stream.write_all(json.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

// ===== client side =====

/// Connect to a running daemon's control socket, send `command`, return its
/// response. Fails clearly if no daemon is listening.
pub async fn send_command(socket_path: &Path, command: &DaemonCommand) -> Result<DaemonResponse> {
    let stream = UnixStream::connect(socket_path).await.with_context(|| {
        format!(
            "connecting to {} — is the daemon running? (`bitrouter start`)",
            socket_path.display()
        )
    })?;
    let mut reader = BufReader::new(stream);
    let mut json = serde_json::to_string(command).context("serialising command")?;
    json.push('\n');
    reader.get_mut().write_all(json.as_bytes()).await?;
    reader.get_mut().flush().await?;

    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .context("reading daemon response")?;
    if line.trim().is_empty() {
        anyhow::bail!("daemon closed the connection without responding");
    }
    serde_json::from_str(line.trim()).context("parsing daemon response")
}

// ===== PID file =====

/// Write the current process id to `path`.
pub async fn write_pid_file(path: &Path) -> Result<()> {
    tokio::fs::write(path, std::process::id().to_string())
        .await
        .with_context(|| format!("writing pid file {}", path.display()))
}

/// Read a process id from a PID file, if it exists and is well-formed.
pub async fn read_pid_file(path: &Path) -> Option<u32> {
    let raw = tokio::fs::read_to_string(path).await.ok()?;
    raw.trim().parse().ok()
}

/// Remove a PID file, ignoring "not found".
pub async fn remove_pid_file(path: &Path) {
    let _ = tokio::fs::remove_file(path).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_round_trip_as_json() {
        for cmd in [
            DaemonCommand::Stop,
            DaemonCommand::Reload,
            DaemonCommand::Status,
            DaemonCommand::Route {
                model: "gpt-5".to_string(),
            },
        ] {
            let json = serde_json::to_string(&cmd).unwrap();
            let back: DaemonCommand = serde_json::from_str(&json).unwrap();
            // tag-based round trip
            assert_eq!(std::mem::discriminant(&cmd), std::mem::discriminant(&back));
        }
    }

    #[test]
    fn responses_round_trip_as_json() {
        let resp = DaemonResponse::Status {
            pid: 42,
            listen: "0.0.0.0:4356".to_string(),
            models: 3,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: DaemonResponse = serde_json::from_str(&json).unwrap();
        match back {
            DaemonResponse::Status { pid, models, .. } => {
                assert_eq!(pid, 42);
                assert_eq!(models, 3);
            }
            other => panic!("expected Status, got {other:?}"),
        }
    }
}
