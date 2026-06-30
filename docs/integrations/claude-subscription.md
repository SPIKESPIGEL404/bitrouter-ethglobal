---
title: Claude subscription
description: Route your Claude Pro or Max plan through BitRouter — OAuth, no Anthropic API key, no per-token bill.
sourceHash: d968900e5a941367a42237a71b676328e40892847db13b3b578b73e969b95289
---

Already paying for Claude Pro or Max? Use that plan as a model source. `bitrouter login anthropic` runs the same OAuth flow Claude Code uses, stores the refreshing token, and attaches it to every request to the `anthropic` provider — so your subscription's usage covers the tokens and there's no `ANTHROPIC_API_KEY` to manage.

<Callout type="info">
**Subscription, not API key.** This is the OAuth path — it bills against your Claude plan. If you'd rather pay per token with an Anthropic API key, skip the login and set `ANTHROPIC_API_KEY` in the environment instead; the same `anthropic` provider picks it up automatically.
</Callout>

## Log in

```bash
bitrouter login anthropic
```

This opens Anthropic's authorize page in your browser (PKCE), and on approval stores the credential under `$XDG_DATA_HOME/bitrouter/oauth-tokens.json`. The token auto-refreshes — you log in once. To check or remove it later:

```bash
bitrouter logout anthropic        # remove the stored credential
```

<Callout type="info">
**Already signed in to Claude Code?** The login menu offers **"Import an existing session from the vendor CLI"** — BitRouter adopts the credential Claude Code already stored (macOS Keychain or `~/.claude/.credentials.json`), with no fresh browser sign-in. The imported token refreshes like any other.
</Callout>

### Multiple accounts

Each credential is keyed by `(provider, label)`. Pass `--label` to keep more than one Claude account side by side:

```bash
bitrouter login anthropic --label work
bitrouter login anthropic --label personal
```

## Route to it

No `bitrouter.yaml` block is required — the `anthropic` provider is built in, and the stored credential is found at request time. Address Claude models by their registry id:

```bash
bitrouter route anthropic:claude-sonnet-4-6
```

Then [start BitRouter and send a request](/docs/integrations/models#start-bitrouter-and-send-a-request). Use the provider-qualified id `anthropic:claude-sonnet-4-6` to pin the request to your subscription, or the bare model name to let BitRouter cascade across active sources.

<Callout type="warn">
**One auth mode per request.** A request uses your OAuth subscription **or** an API key, never both. If a stored login exists it wins; otherwise BitRouter falls back to `ANTHROPIC_API_KEY`. Run `bitrouter logout anthropic` to switch back to key-based billing.
</Callout>

## Learn more

- [Claude Code](/docs/integrations/claude-code) — point the Claude Code *CLI* at BitRouter (a harness), distinct from using your plan as a model source here.
- [Models](/docs/concepts/models) — the full `provider/model` id scheme.
- [Model fallback](/docs/features/model-fallback) — fail over from your subscription to a hosted model on overload.
