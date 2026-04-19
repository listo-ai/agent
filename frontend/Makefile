.PHONY: install dev dev-tauri build-web build-tauri typecheck lint clean

# Install dependencies
install:
	pnpm install

# Start the browser dev server (http://localhost:3000)
dev:
	pnpm dev

# Start Tauri desktop dev (launches native window + hot-reload)
dev-tauri:
	pnpm dev:tauri

# Production build — static SPA
build-web:
	pnpm build:web

# Production build — assets for Tauri bundle
build-tauri:
	pnpm build:tauri

# Build + package the Tauri desktop app
tauri-build:
	pnpm tauri:build

# Type-check without emitting
typecheck:
	pnpm typecheck

# Lint
lint:
	pnpm lint

# Remove build artefacts
clean:
	rm -rf dist-web dist-tauri node_modules
