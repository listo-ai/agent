#!/usr/bin/env bash
# dev/run.sh — start cloud + edge agents + both Studios in one terminal.
#
# Usage: bash dev/run.sh   (or: make dev)
#
# Ctrl-C stops all four processes cleanly.
# No external dependencies — pure bash + standard POSIX utilities.

set -euo pipefail

# Colour prefixes (ANSI). Each process gets its own colour.
C_RESET='\033[0m'
C_CLOUD='\033[34m'   # blue
C_EDGE='\033[32m'    # green
C_SC='\033[35m'      # magenta  (studio-cloud)
C_SE='\033[36m'      # cyan     (studio-edge)

PIDS=()

# prefix_lines COLOR LABEL — reads stdin, prepends "[LABEL] " in color
prefix_lines() {
  local color="$1" label="$2"
  while IFS= read -r line; do
    printf "${color}[%-13s]${C_RESET} %s\n" "$label" "$line"
  done
}

# launch CMD LABEL COLOR — runs CMD in background with prefixed output
launch() {
  local cmd="$1" label="$2" color="$3"
  bash -c "$cmd" 2>&1 | prefix_lines "$color" "$label" &
  PIDS+=($!)
}

cleanup() {
  echo ""
  echo "Stopping all processes..."
  for pid in "${PIDS[@]}"; do
    kill "$pid" 2>/dev/null || true
  done
  wait 2>/dev/null || true
  exit 0
}
trap cleanup INT TERM

# ── resolve repo root (script may be called from anywhere) ───────────────────
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# ── rebuild before launching ─────────────────────────────────────────────────
# Cargo is incremental; a no-op rebuild is fast. This eliminates the
# "I restarted but nothing changed" trap where make dev silently runs
# a stale binary. Opt out with SKIP_REBUILD=1 if you're iterating on
# the frontend only.
if [[ "${SKIP_REBUILD:-0}" != "1" ]]; then
  echo "Rebuilding agent (set SKIP_REBUILD=1 to skip)..."
  cargo build --bin agent
fi

# ── locate the binary ────────────────────────────────────────────────────────
# Prefer debug (what we just rebuilt). Fall back to release only if
# debug is genuinely absent — avoids shadowing a fresh debug build
# with an older release artifact that happens to sit in target/.
if [[ -f target/debug/agent ]]; then
  BIN="target/debug/agent"
elif [[ -f target/release/agent ]]; then
  BIN="target/release/agent"
else
  echo "No agent binary found after build — check cargo output above."
  exit 1
fi

echo "Starting dev environment (Ctrl-C to stop all)..."
echo ""

# ── launch four processes ────────────────────────────────────────────────────
launch "$BIN run --config dev/cloud.yaml --http 127.0.0.1:8081" \
       "cloud:8081" "$C_CLOUD"

launch "$BIN run --config dev/edge.yaml --http 127.0.0.1:8082" \
       "edge:8082" "$C_EDGE"

launch "PUBLIC_AGENT_URL=http://localhost:8081 pnpm --filter @listo/studio dev --port 3002" \
       "studio:3002" "$C_SC"

launch "PUBLIC_AGENT_URL=http://localhost:8082 pnpm --filter @listo/studio dev --port 3010" \
       "studio:3010" "$C_SE"

echo "  cloud agent  → http://localhost:8081"
echo "  edge agent   → http://localhost:8082"
echo "  Studio cloud → http://localhost:3002"
echo "  Studio edge  → http://localhost:3010"
echo ""

# wait for any child to exit unexpectedly
wait -n 2>/dev/null && cleanup || cleanup
