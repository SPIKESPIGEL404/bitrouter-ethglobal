//! Config parsing + `${VAR}` substitution tests.

use super::*;
use crate::language_model::types::ApiProtocol;

#[test]
fn defaults_are_sane() {
    let cfg = Config::default();
    assert_eq!(cfg.server.listen, "0.0.0.0:4356");
    assert!(
        !cfg.server.skip_auth,
        "skip_auth code default must be false"
    );
    assert!(cfg.inherit_defaults);
}

#[test]
fn env_substitution_replaces_vars() {
    let out = substitute_with("api_key: ${BR_TEST_KEY}", |n| {
        (n == "BR_TEST_KEY").then(|| "secret-123".to_string())
    })
    .unwrap();
    assert_eq!(out, "api_key: secret-123");
}

#[test]
fn env_substitution_errors_on_undefined() {
    let err = substitute_with("k: ${MISSING}", |_| None).unwrap_err();
    assert_eq!(err.status(), 400);
}

#[test]
fn env_substitution_handles_multiple_and_literals() {
    let out = substitute_with("a=${A} b=${B} c", |n| Some(format!("<{n}>"))).unwrap();
    assert_eq!(out, "a=<A> b=<B> c");
    assert_eq!(substitute_with("x ${oops", |_| None).unwrap(), "x ${oops");
}

#[test]
fn parses_registry_style_provider() {
    let yaml = r#"
server:
  listen: "127.0.0.1:9000"
  skip_auth: true
providers:
  openai:
    api_base: https://api.openai.com/v1
    api_key: ${BR_CFG_KEY}
    api_protocol:
      - "*": openai
      - "gpt-5*": responses
    rate_limits:
      - "*": { requests_per_minute: 60 }
      - "gpt-5*": { requests_per_minute: 10 }
    models:
      - id: gpt-5
      - id: gpt-4o
      - id: o3
        api_protocol: responses
    tags: [paid]
"#;
    let cfg = parse_with(yaml, |n| (n == "BR_CFG_KEY").then(|| "k-abc".to_string())).unwrap();
    assert_eq!(cfg.server.listen, "127.0.0.1:9000");
    assert!(cfg.server.skip_auth);

    let openai = cfg.providers.get("openai").unwrap();
    assert_eq!(openai.api_key, "k-abc");

    // glob-prefix precedence: `gpt-5*` pattern beats `*`
    assert_eq!(openai.protocol_for("gpt-5"), ApiProtocol::Responses);
    assert_eq!(openai.protocol_for("gpt-4o"), ApiProtocol::Openai);
    // per-model override beats the pattern
    assert_eq!(openai.protocol_for("o3"), ApiProtocol::Responses);

    // rate limits: `gpt-5*` and `*` are independent buckets
    assert_eq!(
        openai.rate_limit_for("gpt-5").unwrap().requests_per_minute,
        Some(10)
    );
    assert_eq!(
        openai.rate_limit_for("gpt-4o").unwrap().requests_per_minute,
        Some(60)
    );
    let bucket_gpt5 = openai.rate_limit_bucket("openai", "gpt-5").unwrap();
    let bucket_4o = openai.rate_limit_bucket("openai", "gpt-4o").unwrap();
    assert_ne!(
        bucket_gpt5, bucket_4o,
        "per-pattern keyed buckets are distinct"
    );
}

#[test]
fn protocol_inference_from_api_base() {
    assert_eq!(
        infer_protocol("https://api.anthropic.com/v1"),
        ApiProtocol::Anthropic
    );
    assert_eq!(
        infer_protocol("https://generativelanguage.googleapis.com/v1beta"),
        ApiProtocol::Google
    );
    assert_eq!(
        infer_protocol("https://api.openai.com/v1"),
        ApiProtocol::Openai
    );
    assert_eq!(
        infer_protocol("https://my-llm.example.com/v1"),
        ApiProtocol::Openai
    );
}

#[test]
fn parses_presets_and_variants() {
    let yaml = r#"
presets:
  careful:
    model: gpt-5
    system_prompt: "Reason carefully."
    params: { temperature: 0.2 }
    routing: { require_tags: [paid], sort: latency }
variants:
  free:
    routing: { require_tags: [free] }
"#;
    let cfg = parse(yaml).unwrap();
    let careful = cfg.presets.get("careful").unwrap();
    assert_eq!(careful.model.as_deref(), Some("gpt-5"));
    assert_eq!(careful.routing.require_tags, vec!["paid"]);
    assert!(cfg.variants.contains_key("free"));
}
