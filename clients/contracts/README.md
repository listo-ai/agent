# /clients/contracts — Wire Contract Fixtures

This directory is the **cross-language source of truth** for the agent wire protocol.

Every client — TypeScript, Rust, Go, Python — reads these fixtures in its round-trip tests.
If a fixture changes it means the wire contract changed; that is a versioned, reviewed event.

## Structure

```
fixtures/
  msg/                  # Msg envelope examples (one JSON file per variant)
  events/               # GraphEvent examples (one JSON file per variant)
  capability-manifest.json   # canonical CapabilityManifest shape
```

## Rules

1. Fixtures are **hand-authored** and **reviewed** — they are the spec, not generated output.
2. A client's round-trip test must deserialise every file in `msg/` and `events/` without error.
3. Adding a fixture file is additive and non-breaking.
4. Changing or removing a fixture file requires a version bump discussion.
