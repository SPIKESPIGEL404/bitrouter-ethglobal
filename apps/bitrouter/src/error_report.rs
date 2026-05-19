//! User-facing CLI diagnostics — replaces anyhow's default
//! `Debug` rendering (`"Error: …\n\nCaused by:\n    …"`) with a tighter,
//! actionable presentation.
//!
//! Shape of an error report:
//!
//! ```text
//! error: <root-cause message, with HTTP-taxonomy prefix stripped>
//!   while: <first context layer>            (one line per layer, oldest-to-newest)
//!   while: …
//!
//!   hint: <how to fix, if we recognise the failure mode>
//!        <…wrapped to subsequent lines>
//! ```
//!
//! Shape of a non-error notice (e.g. the first-run scaffold message):
//!
//! ```text
//! info: <one-liner>
//! ```
//!
//! Colours: red for `error:`, cyan for `info:`, dim for `while:` / `hint:`,
//! emitted only when stderr is a TTY (so piping to a log file stays clean).

use std::io::IsTerminal;

/// ANSI palette. Resolved once per process — picked up from whether
/// stderr is a TTY (the user can override via `NO_COLOR`, which the
/// detection already honours indirectly: `is_terminal` returns false
/// when stderr is redirected).
struct Palette {
    red: &'static str,
    cyan: &'static str,
    bold: &'static str,
    dim: &'static str,
    reset: &'static str,
}

impl Palette {
    fn for_stderr() -> Self {
        // Respect the de-facto `NO_COLOR` convention
        // (<https://no-color.org/>) even when stderr is a TTY.
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        if !no_color && std::io::stderr().is_terminal() {
            Self {
                red: "\x1b[31m",
                cyan: "\x1b[36m",
                bold: "\x1b[1m",
                dim: "\x1b[2m",
                reset: "\x1b[0m",
            }
        } else {
            Self {
                red: "",
                cyan: "",
                bold: "",
                dim: "",
                reset: "",
            }
        }
    }
}

/// Print an anyhow error chain in the documented CLI shape. Always
/// writes to stderr. Caller decides whether to `std::process::exit(1)`
/// afterwards.
pub fn report(err: &anyhow::Error) {
    let p = Palette::for_stderr();
    let chain: Vec<String> = err.chain().map(|e| e.to_string()).collect();
    // The root cause is the most actionable part — surface it as the
    // headline. Anyhow's chain runs head→root; we want the tail.
    let root_raw = chain.last().expect("anyhow chains always have ≥1 entry");
    let root = strip_status_prefix(root_raw);
    eprintln!(
        "{red}{bold}error:{reset} {root}",
        red = p.red,
        bold = p.bold,
        reset = p.reset
    );

    // Intermediate context layers, oldest to newest, skipping the root.
    // (Anyhow's chain is head→root; the head is usually the outermost
    // `.context("loading X")`, which is the most user-facing locator.)
    if chain.len() > 1 {
        for layer in chain[..chain.len() - 1].iter() {
            let stripped = strip_status_prefix(layer);
            eprintln!(
                "  {dim}while:{reset} {stripped}",
                dim = p.dim,
                reset = p.reset
            );
        }
    }

    if let Some(hint) = hint_for(root) {
        eprintln!();
        for line in hint.lines() {
            eprintln!(
                "  {cyan}hint:{reset} {line}",
                cyan = p.cyan,
                reset = p.reset
            );
        }
    }
}

/// Print an informational notice in the same visual family as
/// [`report`]. Used by paths.rs's first-run scaffold message and any
/// future "we did a thing on your behalf" surface.
pub fn info(message: impl std::fmt::Display) {
    let p = Palette::for_stderr();
    eprintln!(
        "{cyan}{bold}info:{reset} {message}",
        cyan = p.cyan,
        bold = p.bold,
        reset = p.reset
    );
}

/// `BitrouterError`'s `Display` impl carries an HTTP-shape prefix
/// (`"bad request: …"`, `"internal error: …"`, etc.) — the CLI doesn't
/// need it. Strip the prefix when we recognise one; leave the message
/// alone otherwise. Keeps the formatter lossless for any error type
/// outside our taxonomy.
fn strip_status_prefix(msg: &str) -> &str {
    const PREFIXES: &[&str] = &[
        "bad request: ",
        "internal error: ",
        "unauthorized: ",
        "forbidden: ",
        "payment required: ",
        "not found: ",
        // `upstream error (…): X` carries the upstream status, which is
        // user-debugging signal — keep that one as-is.
    ];
    for prefix in PREFIXES {
        if let Some(rest) = msg.strip_prefix(prefix) {
            return rest;
        }
    }
    msg
}

/// Recognise a handful of common failure modes and emit an actionable
/// next-step hint. The match is by substring against the *stripped*
/// root message so display-prefix churn doesn't break the table.
fn hint_for(root: &str) -> Option<String> {
    // Undefined config env-var. Pull the var name out so the hint can
    // name it explicitly.
    if let Some(rest) = root.strip_prefix("config references undefined environment variable '") {
        if let Some(var) = rest.strip_suffix("'") {
            return Some(format!(
                "Set `{var}` in your environment (e.g. `export {var}=…`),\n\
                 or remove the `${{{var}}}` reference from bitrouter.yaml."
            ));
        }
    }
    // `-c <missing>` user error.
    if root.contains("does not exist (passed via -c)") {
        return Some(
            "Drop `-c <path>` to use the default resolution order, or run\n\
             `bitrouter init -c <path>` to write a starter config there."
                .into(),
        );
    }
    // BITROUTER_HOME set but file missing.
    if root.contains("BITROUTER_HOME is set to") {
        return Some(
            "Unset `BITROUTER_HOME`, or run\n\
             `bitrouter init -c $BITROUTER_HOME/bitrouter.yaml` to scaffold one."
                .into(),
        );
    }
    // Sqlite path didn't open.
    if root.contains("connecting to database") || root.contains("sqlite") {
        return Some(
            "Check the `database.url` value in bitrouter.yaml. For local use,\n\
             `sqlite://./bitrouter.db` is the default; the file is created on first run."
                .into(),
        );
    }
    // No `$HOME` set (rare; happens in some CI / container shells).
    if root.contains("could not determine home directory") {
        return Some(
            "Either set `BITROUTER_HOME=<dir>` (with a `bitrouter.yaml` inside),\n\
             or pass `-c <path>` explicitly."
                .into(),
        );
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_known_status_prefixes() {
        assert_eq!(strip_status_prefix("bad request: foo"), "foo");
        assert_eq!(strip_status_prefix("internal error: boom"), "boom");
        assert_eq!(strip_status_prefix("not found: route"), "route");
        // Unknown prefix → leave alone.
        assert_eq!(strip_status_prefix("loading /tmp/x"), "loading /tmp/x");
        // Upstream is intentionally preserved — the status is signal.
        assert_eq!(
            strip_status_prefix("upstream error (502): boom"),
            "upstream error (502): boom"
        );
    }

    #[test]
    fn hint_extracts_undefined_env_var_name() {
        let hint = hint_for("config references undefined environment variable 'OPENAI_API_KEY'")
            .expect("hint produced");
        assert!(hint.contains("OPENAI_API_KEY"));
        assert!(hint.contains("export OPENAI_API_KEY"));
    }

    #[test]
    fn hint_recognises_passed_via_dash_c() {
        let hint = hint_for("config file '/x.yaml' does not exist (passed via -c). foo").unwrap();
        assert!(hint.contains("-c <path>"));
    }

    #[test]
    fn hint_recognises_bitrouter_home_missing_file() {
        let hint =
            hint_for("BITROUTER_HOME is set to '/x' but 'bitrouter.yaml' is missing there. foo")
                .unwrap();
        assert!(hint.contains("BITROUTER_HOME"));
    }

    #[test]
    fn hint_returns_none_for_unknown_messages() {
        assert!(hint_for("something we have no opinion on").is_none());
    }
}
