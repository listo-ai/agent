# AGENTS.md

Welcome! This file provides essential context, rules, and instructions for any AI coding agent working on this project.

## 🏗️ Tech Stack & Architecture
- **Backend**: Rust, Cargo workspaces (`crates/`), Tokio, Axum (REST), Tonic (gRPC).
- **Frontend**: TypeScript, React, Rsbuild/Vite (`frontend/`), Shadcn UI.
- **Package Managers**: `cargo` for Rust, `pnpm` for TypeScript/Frontend.

## 🧭 Project Navigation
- `crates/`: Contains all Rust micro-crates (engine, auth, transport, blocks, etc.).
- `frontend/`: Contains the React/TS Studio UI application (`@sys/studio`).
- `clients/`: Contains client SDKs (TypeScript & Rust).
- `dev/`: Contains local SQLite databases and configurations for the agent.
- `Makefile`: The single source of truth for all build, dev, and CI commands.

## 💻 Dev Environment Tips
- **Always use the Makefile** for running tasks instead of guessing manual `cargo` or `pnpm` commands.
- **Start the full dev environment**: Run `make dev` to boot both cloud and edge agents alongside the frontend studio.
- **Start Frontend only**: Run `make frontend` (this will automatically build the TS client first).
- **Check your work**: Run `make check` to verify Rust code compiles quickly without running a full build.
- **Frontend package context**: When running JS commands manually, use `pnpm --filter <pkg_name>`, e.g., `pnpm --filter @sys/studio dev`.

## 🧪 Testing & Linting
- **Full CI Pass**: Run `make ci`. This executes `lint`, `test`, `test-doc`, and `frontend-build`. Your changes must pass this before merging!
- **Rust Testing**: Run `make test` to run all tests via `cargo-nextest`. To test a single crate: `make test-crate CRATE=<name>`.
- **Rust Linting**: Run `make lint` to execute formatting checks and `clippy`.
  - *Note*: We compile with `#![forbid(unsafe_code)]` and `#![deny(unused_must_use)]`.
  - Avoid `.unwrap()` and `.expect()` in production code. Handle errors robustly with `Result`, `thiserror`, and `anyhow`.

## 🎨 UI & Frontend Instructions
- **Shadcn UI**: We use Shadcn UI for components. *Do not write custom CSS or raw Tailwind for standard elements*. Check if a Shadcn component exists first, or use the Shadcn MCP to add it.
- **State & Logic**: Keep UI components clean. Extract complex logic into custom hooks.
- **Package Management**: Always use `pnpm`. Never use `npm` or `yarn` in this workspace.

## 🤖 AI Agent Workflow Rules
1. **Search Before Writing**: Use workspace search (e.g., `grep_search`) to see how similar features or patterns are already implemented (e.g., how Axum routes are structured, how domain entities are serialized).
2. **Incremental Development**: Do not rewrite massive files at once. Make targeted edits.
3. **Run Checks Frequently**: After changing Rust code, always run `make check` or `make lint`. After changing TS, ensure no type errors remain.

## 📚 Companion Stack Guides
- `docs/agents/rust/README.md` for Rust project rules and review checks.
- `docs/agents/typescript/README.md` for shared TypeScript conventions.
- `docs/agents/react/README.md` for React component and hook guidance.
- `docs/agents/shadcn/README.md` for shadcn/ui usage patterns.

## 🔧 Agent CLI

The MCP bootstrap tooling now lives in the standalone `agent-cli` repo (`github.com/NubeDev/agent-cli`), not inside this workspace. It stays agent-agnostic and handles both MCP config sync and stack-specific skills/docs installs.

**Use it through the Makefile**
- `make mcp-init STACK=rust` to write a starter `mcp-compose.yaml`
- `make mcp-sync` to sync `mcp-compose.yaml` into supported agent configs
- `make mcp-test` to health-check configured MCP servers

**Why reuse it?**
- Centralized source-of-truth for MCP services and agent skills.
- Standalone Rust CLI with robust error handling (`anyhow`, `thiserror`).
- The Makefile will use an installed `agent-cli` binary or fall back to `../agent-cli` during local development.
