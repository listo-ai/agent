# Docker / Containerization Overview

## What it is

Docker is how we ship, run, and compose the platform — from a developer's laptop to production clusters. Every piece of the stack has a canonical image, and a single `docker-compose.yml` brings the whole thing up for development or single-tenant deployment.

## The images

One registry (GitHub Container Registry for open-source parts, private registry for enterprise builds). Multi-arch manifests (`amd64` + `arm64`) for everything that runs on edge hardware.

| Image | Base | What's in it | Target arches |
|---|---|---|---|
| `yourapp/agent` | `gcr.io/distroless/cc-debian12` or `alpine` | The single Rust binary — edge + cloud + standalone roles | amd64, arm64, armv7 |
| `yourapp/studio-web` | `nginx:alpine` | Static SPA build, served via nginx | amd64, arm64 |
| `yourapp/block-bacnet` | `distroless/cc` | BACnet block process | amd64, arm64 |
| `yourapp/block-modbus` | `distroless/cc` | Modbus block process | amd64, arm64 |
| `yourapp/block-mqtt` | `distroless/cc` | MQTT block process | amd64, arm64 |
| `zitadel/zitadel` | Upstream | Auth IdP | amd64, arm64 |
| `nats:alpine` | Upstream | NATS server (cluster or leaf) | amd64, arm64 |
| `postgres:17-alpine` | Upstream | Postgres (cloud + dev only) | amd64, arm64 |
| `yourapp/migrator` | `distroless/cc` | Runs SeaORM migrations on startup, then exits | amd64, arm64 |

Everything we build uses **distroless** base images — no shell, no package manager, minimal attack surface, smallest possible footprint.

## Image size targets

| Image | Target size | Why |
|---|---|---|
| `yourapp/agent` | < 50 MB | Pulls fast over slow edge connections |
| Block images | < 30 MB each | Many of them on a gateway; total matters |
| `yourapp/studio-web` | < 20 MB | Static assets only |
| `zitadel/zitadel` | ~ 100 MB | Not ours to control |
| `postgres:17-alpine` | ~ 250 MB | Not ours to control |
| `nats:alpine` | ~ 20 MB | Already tiny |

## Multi-stage builds

Every Rust image uses a multi-stage Dockerfile:

```
Stage 1: builder (rust:1.x) → cargo build --release
Stage 2: distroless → copy binary only
```

Result: the final image contains the compiled binary, its runtime libraries (if glibc) or none (if musl), and nothing else. No build tools, no source, no cargo cache.

## Cross-compilation for ARM

Edge images must build for `aarch64` from our x86 CI. Two options:

| Approach | Pros | Cons |
|---|---|---|
| **Docker buildx with QEMU emulation** | Simple, one command | Slow builds (3–5× native) |
| **`cross` + native ARM runners** | Fast, reliable | More CI setup, needs ARM build agents |

We use **buildx with QEMU** initially, migrate to native ARM runners when CI time becomes painful. This is a standard decision, not a technical one.

## docker-compose — the dev / single-tenant stack

One file at repo root, brings up the entire platform locally. **This is not a cloud production topology** — it's single-node everything. Production cloud uses the Helm charts (NATS cluster with JetStream, managed Postgres, HA Zitadel).

```yaml
# docker-compose.yml — illustrative shape
services:
  # ─── auth ─────────────────────────────────
  zitadel:
    image: zitadel/zitadel:latest
    environment:
      - ZITADEL_DATABASE_POSTGRES_HOST=postgres
      - ZITADEL_EXTERNALDOMAIN=localhost
      - ZITADEL_EXTERNALSECURE=false
    depends_on: [postgres-zitadel]
    ports: ["8010:8010"]

  postgres-zitadel:
    image: postgres:17-alpine
    environment:
      - POSTGRES_DB=zitadel
      - POSTGRES_PASSWORD=dev
    volumes: [zitadel-data:/var/lib/postgresql/data]

  # ─── platform data ───────────────────────
  postgres:
    image: postgres:17-alpine
    environment:
      - POSTGRES_DB=yourapp
      - POSTGRES_PASSWORD=dev
    volumes: [app-data:/var/lib/postgresql/data]
    ports: ["5432:5432"]

  migrator:
    image: yourapp/migrator:dev
    depends_on: [postgres]
    command: ["migrate", "up"]
    restart: "no"

  # ─── messaging ───────────────────────────
  nats:
    image: nats:alpine
    command:
      - "-js"                # enable JetStream (dev / single-tenant only)
      - "-sd=/data"
    volumes: [nats-data:/data]
    ports: ["4222:4222", "8222:8222"]   # standard NATS client port, monitoring port

  # ─── platform ────────────────────────────
  control-plane:
    image: yourapp/agent:dev
    command: ["run", "--role=cloud", "--config=/etc/yourapp/cloud.yaml"]
    depends_on: [migrator, nats, zitadel]
    environment:
      - DATABASE_URL=postgres://yourapp:dev@postgres/yourapp
      - NATS_URL=nats://nats:4225
      - ZITADEL_URL=http://zitadel:8010
    ports: ["3000:3000"]

  edge-agent:
    image: yourapp/agent:dev
    command: ["run", "--role=edge", "--config=/etc/yourapp/edge.yaml"]
    depends_on: [control-plane, nats]
    volumes: [edge-data:/var/lib/yourapp]
    # block processes run as sidecars or inside same container

  studio-web:
    image: yourapp/studio-web:dev
    depends_on: [control-plane]
    ports: ["8000:80"]

volumes:
  app-data:
  zitadel-data:
  nats-data:
  edge-data:
```

One command — `docker compose up` — and a developer has the whole platform running locally with auth, messaging, persistence, the Control Plane, an edge agent, and the web Studio. No manual setup.

## Deployment profiles via Compose overlays

Compose's override pattern gives us different deployment shapes from one base file:

| File | Purpose |
|---|---|
| `docker-compose.yml` | Base — dev / single-tenant local platform |
| `docker-compose.dev.yml` | Local dev additions — volume mounts, hot-reload, verbose logging |
| `docker-compose.edge.yml` | Edge appliance — SQLite instead of Postgres, NATS leaf (Core only, no embedded Zitadel — uses cached JWKS from the Control Plane) |
| `docker-compose.standalone.yml` | Single-tenant on-prem — bundles Zitadel; still single-node |

Cloud production is **not** a compose file — it's the Helm chart in `/charts/yourapp-platform` (NATS cluster, managed/HA Postgres, HA Zitadel).

```bash
docker compose up                                           # dev / single-tenant
docker compose -f docker-compose.yml -f docker-compose.dev.yml up  # dev with hot-reload
docker compose -f docker-compose.standalone.yml up          # on-prem appliance
docker compose -f docker-compose.edge.yml up                # edge (no Zitadel)
```

**Edge never bundles Zitadel.** Edge verifies JWTs against cached JWKS and uses a service-account credential for its own identity — see [AUTH.md](AUTH.md).

## Edge deployment — how the agent runs on a Pi

Docker on the edge is optional but supported. Recommended path:

| Element | Choice |
|---|---|
| Runtime | Docker or Podman |
| Restart policy | `restart: unless-stopped` |
| Memory limit | `mem_limit: 400M` for the agent container on 512 MB gateways (target RSS 350 MB + headroom). `deploy.resources.limits` is Swarm-only and ignored by `docker compose up` — use `mem_limit` for Compose. |
| Volumes | One for SQLite, one for config, one per block for versioned block state (enables rollback per UI.md lifecycle) |
| Network | `host` mode — BACnet/Modbus need real network access |
| Healthcheck (liveness) | `HEALTHCHECK CMD yourapp health --liveness` — process is up, responding |
| Healthcheck (readiness) | `yourapp health --readiness` — JWKS cached, DB open, engine in Running state. Kubernetes maps this to the readiness probe; Compose uses the liveness one. |
| Logs | `json-file` driver with rotation, or forwarded to the Control Plane via NATS |

For really constrained hardware (legacy ARM), we also ship the agent as a standalone binary with a systemd unit — no Docker required. Users pick.

## Block processes in Docker

Three patterns, user's choice:

| Pattern | Trade-off |
|---|---|
| **Sidecar containers** | Each block is its own container, share a volume with the agent for the UDS socket. Clean isolation, cgroup memory caps per block, independent restart. |
| **Inside the agent container** | Blocks as child processes of the agent, using OS-level cgroups within the container. Smaller footprint, no per-block image pull. |
| **Host processes** | Agent runs in Docker, blocks run on host. Rare, for blocks needing hardware access Docker can't give cleanly. |

**Default is sidecar containers** — matches the crash-isolation and per-block resource-ceiling story in the main design. On memory-constrained edge (≤512 MB), the inside-container pattern is the escape hatch when sidecar overhead becomes material; switching is a config change, not a rebuild.

Block-process images are signed with the same cosign + SLSA provenance as platform images (see CI section) and verified by the agent before spawn.

## Networking

| Service | Ports exposed | Internal-only |
|---|---|---|
| Control Plane | 3000 (HTTPS in prod) | — |
| Studio web | 80 / 443 | — |
| NATS | 4222 (clients), 8222 (monitoring) | 6222 (cluster) |
| Zitadel | 8010 | — |
| Postgres | 5432 | yes, in prod |
| Edge agent | none by default; 9000 optional for local admin UI | UDS to blocks is filesystem-scoped |

In production, only the Control Plane and Studio web are exposed to the internet. Everything else is on a private network.

## Configuration

Twelve-factor — config via environment variables with optional YAML file fallback.

| Config source | Precedence |
|---|---|
| Command-line flags | 1 (highest) |
| Environment variables (`YOURAPP_*`) | 2 |
| Config file (`/etc/yourapp/config.yaml`) | 3 |
| Compiled defaults | 4 (lowest) |

Docker images read from env vars by default; mount a config file to override. Secrets are **never** baked into images — always mounted or env-injected at runtime.

## Secrets handling

| Secret | How it's delivered |
|---|---|
| DB passwords | Docker secrets (Swarm), Kubernetes Secrets, or env var from sealed vault |
| Zitadel admin credentials | Same |
| Service account credentials for edge | Mounted as read-only file at `/etc/yourapp/credentials.json` |
| TLS certificates | Mounted from host or cert-manager in k8s |
| JWT signing keys | Never in images — Zitadel owns these |

No secret ever in an image, in `docker-compose.yml` committed to git, or in a Dockerfile `ENV` directive.

## Healthchecks

Every container has one. Examples:

| Service | Healthcheck |
|---|---|
| Agent | `yourapp health` — returns 0 if engine is running |
| Control Plane | HTTP GET on `/api/v1/health` |
| NATS | Built-in `/healthz` on port 8222 |
| Postgres | `pg_isready` |
| Zitadel | HTTP GET on `/debug/healthz` |

Docker uses these to route traffic and trigger restarts. Kubernetes maps them to readiness/liveness probes automatically.

## Logs

All containers log to stdout/stderr as structured JSON (tracing crate's JSON formatter). Docker's default log driver captures to `json-file`; production deployments use `journald`, `fluentd`, or direct-to-Loki/Elasticsearch depending on customer preference.

One consistent log format across all services — single parser, single dashboard, single query language.

## CI / image publishing

| Step | Action |
|---|---|
| PR opens | Build all images, run tests, push to `ghcr.io/yourapp/*:pr-<number>` |
| Merge to main | Build, push to `:main` and `:main-<shortsha>` |
| Tag release | Build, sign with cosign, push to `:v1.2.3` and `:latest` |
| Nightly | Rebuild latest main to pick up base-image security patches |

All production images — **platform AND block-process images** — signed. Customers can verify signatures before pulling. Image attestation via SLSA provenance. The agent verifies block-process image signatures before spawning the sidecar.

## Kubernetes

Docker Compose for dev and single-tenant. Kubernetes for multi-tenant cloud. Helm chart per major component:

| Chart | Contents |
|---|---|
| `yourapp-platform` | Control Plane + migrator + ingress |
| `yourapp-nats` | NATS cluster with JetStream and PVCs |
| `yourapp-zitadel` | Zitadel deployment + Postgres subchart |
| `yourapp-edge` | Edge agent DaemonSet (for customers running k8s at the edge) |

Helm values override per environment. Operators for production — Zitadel's operator handles identity, a small custom operator handles fleet orchestration of edge agents.

## Local dev experience

`docker compose up --build` gives a developer:

- Full platform running in ~30 seconds after first build
- Hot-reload for Studio (via dev-mode volume mount + rsbuild dev server)
- Hot-reload for Rust (via `cargo watch` in dev override, slower but works)
- Pre-seeded Zitadel with a dev user (`admin@dev.local` / `dev`)
- Pre-seeded Postgres with test data
- Access to all services on localhost with consistent ports

## What's explicitly NOT done

- **No custom orchestrator.** Docker Compose for simple, Kubernetes for complex. We don't ship our own.
- **No fat "all-in-one" image.** Separate images compose together; no 2 GB monolith.
- **No `:latest` in production.** Always pinned tags. `:latest` exists only for dev convenience.
- **No shells in production images.** Distroless base, no `docker exec /bin/sh` for debugging. Use logs and metrics instead.
- **No secrets in image layers.** Ever. Secrets-scanner runs in CI and fails the build.

## Stage in the coding plan

Docker comes in gradually:

- **Stage 0:** `Dockerfile` for the agent, compose file with Postgres + NATS
- **Stage 4:** Migrator image + volume persistence
- **Stage 5:** Multi-arch builds for ARM
- **Stage 7:** Zitadel in compose, full auth in dev
- **Stage 14:** Helm charts for cloud deployment
- **Stage 15:** Signed images, SLSA provenance, nightly rebuilds

## One-line summary

**Multi-arch distroless images for every Rust binary, upstream images for Zitadel/NATS/Postgres, one `docker-compose.yml` that brings the whole platform up locally, with Helm charts and operators for cloud — same images from a developer's laptop to a 10,000-agent production fleet, built and signed by CI with secrets never in images.**