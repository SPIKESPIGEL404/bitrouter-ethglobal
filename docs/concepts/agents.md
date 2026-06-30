---
title: Agents
description: What agent-native means on BitRouter — the ACP gateway for identity, discovery, and task dispatch, plus KYA identity for autonomous pay-per-use.
sourceHash: de5b0c53a66dfa96e4e6012ed86e7a10216cc7254e3e2b316911b1ef0429cfd3
---

BitRouter is **agent-native**: the primitives below assume the caller is an autonomous agent, not a human at a keyboard. That shows up in two places — how agents are identified and reached, and how they pay.

## The ACP gateway — identity and dispatch

Just as the MCP gateway lets an agent reach many tool servers, the **ACP gateway** handles the agent side: **agent identity, discovery, and task dispatch** across hosts. It's how an agent gets a place in the network, can be found, and can hand off or receive tasks — through the same single-endpoint model BitRouter uses everywhere.

## KYA — verifiable identity that can pay

An autonomous agent holding your keys is a liability unless it has an identity of its own. **KYA (Know-Your-Agent)** gives an agent a **verifiable identity**, which is what makes autonomous payment safe: with that identity, an agent can **pay per use** through the Machine Payment Protocol — x402/MPP — settling each request itself, with no credit cards, prepaid credits, or invoices in the loop.

## Learn how to

- [Agentic payment](/docs/cloud/payment) — autonomous pay-per-use via MPP / x402.
