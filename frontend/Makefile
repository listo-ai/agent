.PHONY: install dev dev-tauri build-web build-tauri typecheck lint clean

# nvm installs node (and pnpm) under ~/.nvm; make runs with a plain PATH so
# we prepend the active node bin directory explicitly.
export PATH := $(HOME)/.nvm/versions/node/v22.22.0/bin:$(PATH)

PNPM := pnpm

# Install dependencies
install:
	$(PNPM) install

# Start the browser dev server (http://localhost:3000)
dev:
	$(PNPM) dev

# Start Tauri desktop dev (launches native window + hot-reload)
dev-tauri:
	$(PNPM) dev:tauri

# Production build — static SPA
build-web:
	$(PNPM) build:web

# Production build — assets for Tauri bundle
build-tauri:
	$(PNPM) build:tauri

# Build + package the Tauri desktop app
tauri-build:
	$(PNPM) tauri:build

# Type-check without emitting
typecheck:
	$(PNPM) typecheck

# Lint
lint:
	$(PNPM) lint

# Remove build artefacts
clean:
	rm -rf dist-web dist-tauri node_modules
