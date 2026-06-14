---
title: BYOK Connect Service
slug: byok-connect-service
topic: nostr-protocol
summary: BYOKConnectService has a preconditionFailure when no UIWindowScene is available, serving as a developer-contract guard.
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-13
updated: 2026-06-14
verified: 2026-05-13
compiled-from: conversation
sources:
  - session:0f3f24f7-54de-49f8-b160-a92f735f6a00
  - session:rollout-2026-05-09T14-56-23-019e0c98-8803-7ef0-b7a2-bf0b605a2360
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# BYOK Connect Service

## Precondition Checks

BYOKConnectService has a preconditionFailure when no UIWindowScene is available, serving as a developer-contract guard. <!-- [^0f3f2-25] -->

## Provider Support

BYOK supports Ollama Cloud as a first-class provider with scope 'key:ollama'. <!-- [^rollo-8] -->

## Connect Flow & Key Storage

The podcast player can request an Ollama Cloud API key through BYOK via a PKCE connect flow, with BYOK/manual/none metadata tracked and raw keys stored only in Keychain. <!-- [^rollo-9] -->

## Raw Key Reveal Endpoint

BYOK's raw key reveal endpoint uses POST only (not GET) to avoid browser/query log exposure, is authenticated, no-store, and returns the raw key only for keys owned by the signed-in user. <!-- [^rollo-10] -->

## Key Editor UI

BYOK's key editor UI has a raw key reveal panel with Show/Hide/Copy, and does not persist the raw key in app state. <!-- [^rollo-11] -->

## Provider Credentials

Connected_at timestamps on provider credentials are stamped by the kernel clock (D9 doctrine), not by shell wall-clock time; the field is dropped from both iOS and Android payloads. <!-- [^c1691-431] -->
