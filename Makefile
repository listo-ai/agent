# Makefile — Rust backend + frontend
# Requires: cargo, cross (for cross-compilation), cargo-nextest (for tests), pnpm
#
# Roles (passed via --role at runtime): cloud | edge | standalone
# Build profiles: debug (default) | release

.DEFAULT_GOAL := help
SHELL         := /bin/bash

BIN           := agent
CARGO         := cargo
CROSS         := cross
# pnpm and node are symlinked into ~/.local/bin (stable across nvm version switches).
export PATH := $(HOME)/.local/bin:$(PATH)

PNPM          := pnpm
CLIENT_PKG    := @sys/agent-client
FRONTEND_DIR  := frontend

# ── release flag ──────────────────────────────────────────────────────────────
ifdef RELEASE
  PROFILE_FLAG := --release
  PROFILE_DIR  := release
else
  PROFILE_FLAG :=
  PROFILE_DIR  := debug
endif

# ── cross-compile targets (from OVERVIEW.md) ──────────────────────────────────
TARGET_EDGE_ARM64       := aarch64-unknown-linux-gnu
TARGET_EDGE_ARM64_MUSL  := aarch64-unknown-linux-musl
TARGET_EDGE_ARM32       := armv7-unknown-linux-gnueabihf
TARGET_X86              := x86_64-unknown-linux-gnu
TARGET_X86_MUSL         := x86_64-unknown-linux-musl

OUT_DIR := target

# ──────────────────────────────────────────────────────────────────────────────
.PHONY: help
help: ## Show this help
	@awk 'BEGIN{FS=":.*##"} /^[a-zA-Z_-]+:.*##/{printf "  \033[36m%-28s\033[0m %s\n",$$1,$$2}' $(MAKEFILE_LIST)

# ── install / bootstrap ──────────────────────────────────────────────────────
.PHONY: install
install: ## Install all JS/TS dependencies (pnpm workspaces)
	$(PNPM) install

.PHONY: build-client
build-client: ## Compile @sys/agent-client → clients/ts/dist/
	$(PNPM) --filter $(CLIENT_PKG) build

# ── dev ───────────────────────────────────────────────────────────────────────
.PHONY: build
build: ## Build agent (debug by default; RELEASE=1 for release)
	$(CARGO) build $(PROFILE_FLAG) --bin $(BIN)

.PHONY: run
run: ## Run edge agent on :8080 using dev/edge.yaml + dev/edge.db (RELEASE=1 for release)
	$(CARGO) run $(PROFILE_FLAG) --bin $(BIN) -- run --config dev/edge.yaml --http 127.0.0.1:8080

.PHONY: frontend
frontend: build-client ## Start the Rsbuild dev server (builds client first)
	$(PNPM) --filter @sys/studio dev

.PHONY: frontend-build
frontend-build: build-client ## Production web build of the Studio UI
	$(PNPM) --filter @sys/studio build:web

# ── two-agent dev env (cloud + edge side-by-side) ────────────────────────────
# See dev/README.md for the full port map and rationale.

.PHONY: dev
dev: build build-client ## Start cloud + edge + both Studios (Ctrl-C stops all). Rebuilds agent + TS client first.
	@bash dev/run.sh

.PHONY: run-cloud
run-cloud: ## Run the cloud agent on 127.0.0.1:8081 (config: dev/cloud.yaml)
	$(CARGO) run $(PROFILE_FLAG) --bin $(BIN) -- run --config dev/cloud.yaml --http 127.0.0.1:8081

.PHONY: run-edge
run-edge: ## Run the edge agent on 127.0.0.1:8082 (config: dev/edge.yaml)
	$(CARGO) run $(PROFILE_FLAG) --bin $(BIN) -- run --config dev/edge.yaml --http 127.0.0.1:8082

.PHONY: studio-cloud
studio-cloud: build-client ## Start Studio pointed at the cloud agent (http://localhost:3001)
	PUBLIC_AGENT_URL=http://localhost:8081 \
	  $(PNPM) --filter @sys/studio dev --port 3001

.PHONY: studio-edge
studio-edge: build-client ## Start Studio pointed at the edge agent (http://localhost:3002)
	PUBLIC_AGENT_URL=http://localhost:8082 \
	  $(PNPM) --filter @sys/studio dev --port 3002

.PHONY: dev-reset
dev-reset: ## Wipe dev/ databases and staged plugins (keeps configs)
	rm -f dev/cloud.db dev/cloud.db-shm dev/cloud.db-wal
	rm -f dev/edge.db  dev/edge.db-shm  dev/edge.db-wal
	rm -rf dev/cloud-plugins/* dev/edge-plugins/*
	@echo "dev/ reset — next boot will seed fresh graphs."

# ── check / lint ──────────────────────────────────────────────────────────────
.PHONY: check
check: ## cargo check (all workspace crates)
	$(CARGO) check --workspace

.PHONY: clippy
clippy: ## Run clippy with workspace lints
	$(CARGO) clippy --workspace --all-targets -- -D warnings

.PHONY: fmt
fmt: ## Format all sources
	$(CARGO) fmt --all

.PHONY: fmt-check
fmt-check: ## Check formatting without writing changes
	$(CARGO) fmt --all -- --check

.PHONY: lint
lint: fmt-check clippy ## fmt-check + clippy

# ── test ──────────────────────────────────────────────────────────────────────
.PHONY: test
test: ## Run all workspace tests (cargo nextest if available, else cargo test)
	@if command -v cargo-nextest &>/dev/null || $(CARGO) nextest --version &>/dev/null 2>&1; then \
	  $(CARGO) nextest run --workspace; \
	else \
	  $(CARGO) test --workspace; \
	fi

.PHONY: test-doc
test-doc: ## Run doc-tests only
	$(CARGO) test --workspace --doc

.PHONY: test-crate
test-crate: ## Run tests for a single crate: make test-crate CRATE=engine
	$(CARGO) test -p $(CRATE)

# ── release builds ────────────────────────────────────────────────────────────
.PHONY: release
release: ## Build optimised agent for the host target
	$(CARGO) build --release --bin $(BIN)

# ── cross-compilation ─────────────────────────────────────────────────────────
.PHONY: cross-edge-arm64
cross-edge-arm64: ## Cross-compile release agent → aarch64 (Pi 4/5, gateways)
	$(CROSS) build --release --bin $(BIN) --target $(TARGET_EDGE_ARM64)

.PHONY: cross-edge-arm64-musl
cross-edge-arm64-musl: ## Cross-compile release agent → aarch64-musl (static / Alpine)
	$(CROSS) build --release --bin $(BIN) --target $(TARGET_EDGE_ARM64_MUSL)

.PHONY: cross-edge-arm32
cross-edge-arm32: ## Cross-compile release agent → armv7 (legacy ARM gateways)
	$(CROSS) build --release --bin $(BIN) --target $(TARGET_EDGE_ARM32)

.PHONY: cross-x86
cross-x86: ## Cross-compile release agent → x86_64 (cloud / x86 gateways)
	$(CROSS) build --release --bin $(BIN) --target $(TARGET_X86)

.PHONY: cross-x86-musl
cross-x86-musl: ## Cross-compile release agent → x86_64-musl (static / scratch containers)
	$(CROSS) build --release --bin $(BIN) --target $(TARGET_X86_MUSL)

.PHONY: cross-all
cross-all: cross-edge-arm64 cross-edge-arm64-musl cross-edge-arm32 cross-x86 cross-x86-musl ## Build all cross-compile targets

# ── docs ──────────────────────────────────────────────────────────────────────
.PHONY: doc
doc: ## Build rustdoc for the whole workspace
	$(CARGO) doc --workspace --no-deps

.PHONY: doc-open
doc-open: ## Build and open rustdoc in a browser
	$(CARGO) doc --workspace --no-deps --open

# ── CI ────────────────────────────────────────────────────────────────────────
.PHONY: ci
ci: lint test test-doc frontend-build ## Full CI pass: lint + tests + doc-tests + frontend build

# ── clean ─────────────────────────────────────────────────────────────────────
.PHONY: clean
clean: ## Remove build artefacts
	$(CARGO) clean
