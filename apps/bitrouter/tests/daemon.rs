//! Integration tests for the Unix-socket daemon control surface (007 §6.1):
//! roundtrip `Status` / `Route` / `Reload` / `Stop` against a fully assembled
//! `App`. Bare-bones — no HTTP server, just the control socket.

use std::sync::Arc;
use std::time::Duration;

use bitrouter::build_app_with_path;
use bitrouter::daemon::{self, DaemonCommand, DaemonResponse};
use bitrouter_sdk::config;

fn tiny_config_yaml(db_url: &str) -> String {
    // Two providers declare overlapping models so Route returns a real chain.
    format!(
        r#"
server:
  listen: "127.0.0.1:0"
  skip_auth: true
database:
  url: "{db_url}"
providers:
  openai:
    api_base: https://api.openai.com/v1
    api_key: k1
    models: [{{ id: gpt-5 }}, {{ id: shared }}]
  anthropic:
    api_base: https://api.anthropic.com/v1
    api_key: k2
    models: [{{ id: shared }}]
"#
    )
}

/// Write a tiny config to a temp file and return its path (so `build_app_with_path`
/// can record it for `reload`).
async fn write_config(dir: &std::path::Path, db_url: &str) -> std::path::PathBuf {
    tokio::fs::create_dir_all(dir).await.unwrap();
    let path = dir.join("bitrouter.yaml");
    tokio::fs::write(&path, tiny_config_yaml(db_url))
        .await
        .unwrap();
    path
}

/// Build a fresh tempdir scoped to this test run.
fn tempdir(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "br-daemon-{tag}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[tokio::test]
async fn status_route_and_stop_roundtrip_over_the_control_socket() {
    let dir = tempdir("status");
    let cfg_path = write_config(&dir, "sqlite::memory:").await;
    let cfg = config::load(&cfg_path).await.unwrap();
    let assembled = build_app_with_path(&cfg, Some(&cfg_path)).await.unwrap();
    let app = Arc::new(assembled.app);

    let socket = dir.join("bitrouter.sock");
    let server = tokio::spawn(daemon::run_control_socket(
        socket.clone(),
        app.clone(),
        "127.0.0.1:1234".to_string(),
    ));

    // Wait for the listener to be ready (bind is fast but not synchronous).
    for _ in 0..50 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Status → reports a real model count from the routing table.
    let status = daemon::send_command(&socket, &DaemonCommand::Status)
        .await
        .unwrap();
    match status {
        DaemonResponse::Status { listen, models, .. } => {
            assert_eq!(listen, "127.0.0.1:1234");
            assert_eq!(models, 2, "gpt-5 + shared");
        }
        other => panic!("expected Status, got {other:?}"),
    }

    // Route → returns the cascade chain (anthropic first, then openai).
    let route = daemon::send_command(
        &socket,
        &DaemonCommand::Route {
            model: "shared".to_string(),
        },
    )
    .await
    .unwrap();
    match route {
        DaemonResponse::Route { chain } => {
            assert_eq!(chain.len(), 2);
            assert_eq!(chain[0].provider, "anthropic");
            assert_eq!(chain[1].provider, "openai");
        }
        other => panic!("expected Route, got {other:?}"),
    }

    // Stop → server returns and the socket file is removed.
    let stop = daemon::send_command(&socket, &DaemonCommand::Stop)
        .await
        .unwrap();
    assert!(matches!(stop, DaemonResponse::Ok));
    server.await.unwrap().unwrap();
    assert!(
        !socket.exists(),
        "socket file should be removed on shutdown"
    );

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn reload_re_reads_the_config_file() {
    let dir = tempdir("reload");
    let cfg_path = write_config(&dir, "sqlite::memory:").await;
    let cfg = config::load(&cfg_path).await.unwrap();
    let assembled = build_app_with_path(&cfg, Some(&cfg_path)).await.unwrap();
    let app = Arc::new(assembled.app);

    let socket = dir.join("bitrouter.sock");
    let server = tokio::spawn(daemon::run_control_socket(
        socket.clone(),
        app.clone(),
        "127.0.0.1:0".to_string(),
    ));
    for _ in 0..50 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Rewrite the config to drop the anthropic provider.
    let new_yaml = r#"
server:
  listen: "127.0.0.1:0"
  skip_auth: true
database:
  url: "sqlite::memory:"
providers:
  openai:
    api_base: https://api.openai.com/v1
    api_key: k1
    models: [{ id: gpt-5 }, { id: shared }]
"#;
    tokio::fs::write(&cfg_path, new_yaml).await.unwrap();

    let resp = daemon::send_command(&socket, &DaemonCommand::Reload)
        .await
        .unwrap();
    assert!(matches!(resp, DaemonResponse::Ok));

    // After reload, `shared` resolves to one hop (openai), not two.
    let route = daemon::send_command(
        &socket,
        &DaemonCommand::Route {
            model: "shared".to_string(),
        },
    )
    .await
    .unwrap();
    match route {
        DaemonResponse::Route { chain } => {
            assert_eq!(chain.len(), 1, "anthropic should be gone after reload");
            assert_eq!(chain[0].provider, "openai");
        }
        other => panic!("expected Route, got {other:?}"),
    }

    // Cleanup
    let _ = daemon::send_command(&socket, &DaemonCommand::Stop).await;
    let _ = server.await;
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn route_for_unknown_model_returns_a_clean_error() {
    let dir = tempdir("noroute");
    let cfg_path = write_config(&dir, "sqlite::memory:").await;
    let cfg = config::load(&cfg_path).await.unwrap();
    let assembled = build_app_with_path(&cfg, Some(&cfg_path)).await.unwrap();
    let app = Arc::new(assembled.app);

    let socket = dir.join("bitrouter.sock");
    let server = tokio::spawn(daemon::run_control_socket(
        socket.clone(),
        app.clone(),
        "127.0.0.1:0".to_string(),
    ));
    for _ in 0..50 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let resp = daemon::send_command(
        &socket,
        &DaemonCommand::Route {
            model: "no-such-model".to_string(),
        },
    )
    .await
    .unwrap();
    match resp {
        DaemonResponse::Error { message } => {
            assert!(message.contains("no-such-model") || message.to_lowercase().contains("model"));
        }
        other => panic!("expected Error, got {other:?}"),
    }

    let _ = daemon::send_command(&socket, &DaemonCommand::Stop).await;
    let _ = server.await;
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn client_fails_clearly_when_no_daemon_is_listening() {
    // Path that definitely doesn't exist.
    let bogus = std::env::temp_dir().join(format!(
        "no-bitrouter-{}.sock",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let err = daemon::send_command(&bogus, &DaemonCommand::Status)
        .await
        .unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("daemon running") || msg.contains("connecting to"),
        "expected a helpful error, got: {msg}"
    );
}
