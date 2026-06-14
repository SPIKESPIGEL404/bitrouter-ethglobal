//! A minimal ACP client: spawns an agent, drives one prompt to completion, and
//! collects the final message, tool-call targets, and stop reason. Newline-
//! delimited JSON-RPC 2.0 over the child's stdio.

use std::collections::BTreeMap;
use std::process::Stdio;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use bitrouter_sdk::{BitrouterError, Result};

/// Outcome of driving one ACP prompt.
#[derive(Debug, Clone, Default)]
pub struct SessionOutcome {
    /// Concatenated agent message text.
    pub final_message: String,
    /// Tool-call titles seen (proxy for files touched / actions).
    pub tool_calls: Vec<String>,
    /// The terminal `stopReason`, if the agent sent one.
    pub stop_reason: Option<String>,
}

/// How to launch the worker.
pub struct WorkerSpawn {
    /// Executable (e.g. `opencode`).
    pub command: String,
    /// Args (e.g. `["acp", "--cwd", "<abs>"]`).
    pub args: Vec<String>,
    /// Extra env (e.g. `OPENCODE_CONFIG`).
    pub env: BTreeMap<String, String>,
}

/// Spawn the worker, run `initialize → session/new → session/prompt(task)`, and
/// collect the outcome. `kill_on_drop` guarantees teardown.
pub async fn drive_once(spawn: WorkerSpawn, task: &str) -> Result<SessionOutcome> {
    let mut cmd = Command::new(&spawn.command);
    cmd.args(&spawn.args)
        .envs(&spawn.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    let mut child: Child = cmd
        .spawn()
        .map_err(|e| BitrouterError::internal(format!("spawning '{}': {e}", spawn.command)))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| BitrouterError::internal("no stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| BitrouterError::internal("no stdout"))?;
    let mut lines = BufReader::new(stdout).lines();

    let mut next_id = 1i64;
    send_request(
        &mut stdin,
        &mut next_id,
        "initialize",
        json!({
            "protocolVersion": 1,
            "clientCapabilities": {
                "fs": { "readTextFile": true, "writeTextFile": true },
                "terminal": true
            }
        }),
    )
    .await?;
    wait_for_result(&mut lines, 1).await?;

    send_request(
        &mut stdin,
        &mut next_id,
        "session/new",
        json!({ "cwd": ".", "mcpServers": [] }),
    )
    .await?;
    let new_res = wait_for_result(&mut lines, 2).await?;
    let session_id = new_res
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BitrouterError::internal("session/new: no sessionId"))?
        .to_string();

    send_request(
        &mut stdin,
        &mut next_id,
        "session/prompt",
        json!({
            "sessionId": session_id,
            "prompt": [{ "type": "text", "text": task }]
        }),
    )
    .await?;

    let mut outcome = SessionOutcome::default();
    loop {
        let line = match lines
            .next_line()
            .await
            .map_err(|e| BitrouterError::internal(format!("read: {e}")))?
        {
            Some(l) => l,
            None => break,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Server-initiated request: has both id and method. Answer defensively.
        if msg.get("id").is_some() && msg.get("method").is_some() {
            let id = msg.get("id").cloned().unwrap_or(Value::Null);
            let reply = json!({ "jsonrpc": "2.0", "id": id, "result": {} });
            let _ = stdin.write_all(format!("{}\n", reply).as_bytes()).await;
            continue;
        }
        // Response to our session/prompt request: has id, has result.
        if msg.get("id").is_some() && msg.get("result").is_some() {
            outcome.stop_reason = msg["result"]
                .get("stopReason")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            break;
        }
        // Notification: has method, no id.
        if msg.get("method").and_then(|m| m.as_str()) == Some("session/update") {
            let u = &msg["params"]["update"];
            match u.get("sessionUpdate").and_then(|v| v.as_str()) {
                Some("agent_message_chunk") => {
                    if let Some(t) = u["content"]["text"].as_str() {
                        outcome.final_message.push_str(t);
                    }
                }
                Some("tool_call") => {
                    if let Some(title) = u.get("title").and_then(|v| v.as_str()) {
                        outcome.tool_calls.push(title.to_string());
                    }
                }
                _ => {}
            }
        }
    }
    let _ = child.kill().await;
    Ok(outcome)
}

async fn send_request(
    stdin: &mut ChildStdin,
    next_id: &mut i64,
    method: &str,
    params: Value,
) -> Result<()> {
    let id = *next_id;
    *next_id += 1;
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    stdin
        .write_all(format!("{}\n", msg).as_bytes())
        .await
        .map_err(|e| BitrouterError::internal(format!("write {method}: {e}")))?;
    Ok(())
}

async fn wait_for_result(lines: &mut Lines<BufReader<ChildStdout>>, id: i64) -> Result<Value> {
    loop {
        let line = lines
            .next_line()
            .await
            .map_err(|e| BitrouterError::internal(format!("read: {e}")))?
            .ok_or_else(|| BitrouterError::internal("agent closed stdout before responding"))?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if msg.get("id").and_then(|v| v.as_i64()) == Some(id) {
            if let Some(err) = msg.get("error") {
                return Err(BitrouterError::internal(format!(
                    "agent error on id {id}: {err}"
                )));
            }
            return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn drives_fake_agent_to_end_turn() {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/fake_acp_agent.mjs"
        );
        let mut env = BTreeMap::new();
        env.insert("NODE_NO_WARNINGS".to_string(), "1".to_string());
        let spawn = WorkerSpawn {
            command: "node".to_string(),
            args: vec![fixture.to_string()],
            env,
        };
        let out = drive_once(spawn, "do the task at /tmp/out.txt")
            .await
            .unwrap();
        assert_eq!(out.stop_reason.as_deref(), Some("end_turn"));
        assert!(out.final_message.contains("done"));
        assert_eq!(out.tool_calls, vec!["write /tmp/out.txt".to_string()]);
    }
}
