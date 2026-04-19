# `dev/` — local two-agent dev environment

Side-by-side cloud + edge agents + two Studio instances on one machine.
Matches the topology of a real deployment closely enough to catch
cross-agent bugs (fleet transport, subject namespacing, UI differences)
without spinning up actual infrastructure.

## Layout

```
dev/
├── cloud.yaml              agent config: role=cloud, HTTP 8081
├── edge.yaml               agent config: role=edge,  HTTP 8082
├── cloud.db                sqlite — created on first run, .gitignored
├── edge.db                 sqlite — created on first run, .gitignored
├── cloud-plugins/          plugins the cloud agent scans
└── edge-plugins/           plugins the edge agent scans
```

## Port map

| Component           | Port  | URL                          |
|---------------------|-------|------------------------------|
| Cloud agent (REST)  | 8081  | http://localhost:8081        |
| Edge agent  (REST)  | 8082  | http://localhost:8082        |
| Studio → cloud      | 3001  | http://localhost:3001        |
| Studio → edge       | 3002  | http://localhost:3002        |

Port 8080 is used by `make run` — a single edge agent using `dev/edge.yaml`
and `dev/edge.db`. Use it for everyday dev; use `make dev` when you need
both agents talking to each other.

## Quickstart

```bash
# Single edge agent on :8080 — everyday dev:
make run

# Full cloud + edge + both Studios, colour-coded logs, Ctrl-C stops all:
make dev

# Or start each piece manually:
make run-cloud
make run-edge
make studio-cloud
make studio-edge
```

`make dev` runs `dev/run.sh` — a self-contained bash script, no extra tools needed.

## Staging plugins per agent

```bash
# Stage into cloud:
make -C examples/plugin-hello install-plugin \
    PLUGINS_DIR=$(pwd)/dev/cloud-plugins

# Stage into edge:
make -C examples/plugin-hello install-plugin \
    PLUGINS_DIR=$(pwd)/dev/edge-plugins
```

Then click **Rescan** in that Studio's Plugins page, or
`curl -X POST http://localhost:8081/api/v1/plugins/reload`.

## Resetting state

```bash
make dev-reset
```

Removes `dev/cloud.db`, `dev/edge.db` (and WAL files), and empties both
plugin dirs. The agents will seed a fresh graph on their next boot.
