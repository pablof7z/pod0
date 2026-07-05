# Provider Cassette Replay

Provider cassettes are redacted request/response envelopes for validation paths
that normally depend on external LLM, STT, TTS, model-catalog, embedding, search,
or relay services. They exist so an agent can exercise provider-backed product
flows without live credentials and still collect screenshots, UX critique, and
performance evidence.

## Current Coverage

Initial cassette fixtures live under `tests/fixtures/provider_cassettes/` and
cover:

- OpenRouter chat completion for transcript-grounded agent answers.
- Ollama chat completion for local/cloud agent answers.
- OpenRouter embeddings for knowledge-search chunks.
- OpenRouter Whisper transcription.
- ElevenLabs Scribe transcription.
- AssemblyAI transcription.
- Perplexity online search with sources.

## Verification

Run:

```bash
cargo run -p nmp-app-podcast --bin provider-cassettes -- verify tests/fixtures/provider_cassettes
```

The verifier checks schema version, unique IDs, required scenario references,
NMP doctrine coverage tags, deterministic request body hashes, deterministic
request fingerprints, redaction markers, and premium-app latency budgets.

## Replay Contract

Each cassette uses this matching key:

- provider
- operation
- HTTP method
- URL
- canonical request body SHA-256

Authorization headers and raw secrets are excluded from matching and must never
appear in cassette files. Multipart requests represent private media with an
`audio_sha256` field instead of storing raw bytes.

## Runtime Replay

Set `POD0_PROVIDER_CASSETTE_DIR` to the fixture directory to enable runtime
replay:

```bash
POD0_PROVIDER_CASSETTE_DIR=tests/fixtures/provider_cassettes \
  cargo test -p nmp-app-podcast provider_replay --lib
```

Replay is wired through the Rust provider transports for OpenRouter/Ollama
chat completions, OpenRouter embeddings, OpenRouter Whisper, ElevenLabs Scribe,
AssemblyAI transcripts, and Perplexity search. When replay mode is enabled, a
request miss fails closed instead of falling through to live provider network
calls.

Multipart STT replay uses redacted cassette audio references. Validation flows
should pass `cassette://` URLs with the recorded hash in the query string, for
example:

```text
cassette://audio/pod0-validation-short.wav?sha256=5b4f0f8fb8d78f4fffb4f06f4ed0a9b41476c5d550da625a5a2db7c2d6a17f0f
```

The runtime strips that hash into the semantic request body as `audio_sha256`;
it never reads or stores the private audio bytes in the cassette.
