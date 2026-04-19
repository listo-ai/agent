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

Port 8080 stays reserved for `make run` (the default single-agent flow
used by most existing docs).

## Quickstart

```bash
# One command, everything up, colour-coded logs, Ctrl-C stops all:
make dev

# Or run each piece in its own terminal:
make run-cloud
make run-edge
make studio-cloud
make studio-edge
```

`make dev` requires [`overmind`](https://github.com/DarthSim/overmind)
(or `hivemind`). On macOS: `brew install overmind`. On Linux:
`go install github.com/DarthSim/overmind/v2@latest`. If you don't have
it, start each process in its own terminal with the four targets above.

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
rm -f dev/cloud.db dev/edge.db         # wipe graphs
rm -rf dev/cloud-plugins/* dev/edge-plugins/*   # unstage plugins
```

The agents will seed a fresh station + `/agent/plugins/` subtree on
their next boot.
