# Slot History — CLI Reference

Status: implemented and tested (Stages 5–6 of [SLOT-STORAGE.md](SLOT-STORAGE.md))
Surfaces: `agent slots history *`, `agent slots telemetry *`, REST `/api/v1/history`, `/api/v1/telemetry`

---

## Prerequisites

The agent must be started with a `--db` path for history endpoints to be available.
Without a DB, all history and telemetry endpoints return `503 Service Unavailable`.

```sh
./agent run --role standalone --db /path/to/agent.db
```

---

## Storage split — which command for which slot type

| Slot value type | Store | CLI command group |
|---|---|---|
| `Bool` | Scalar time-series (`slot_timeseries`) | `slots telemetry` |
| `Number` | Scalar time-series (`slot_timeseries`) | `slots telemetry` |
| `String` | Structured history (`slot_history`) | `slots history` |
| `Json` (object/array) | Structured history (`slot_history`) | `slots history` |
| `Binary` | Structured history (`slot_history`) | (REST only, no CLI yet) |

Use `slots history` for text/JSON slots. Use `slots telemetry` for numeric/boolean sensors.
Querying the wrong command for a slot type returns `{"data":[]}` (empty result), not an error.

---

## `slots history` — structured history (String / Json / Binary)

### Record the current value on-demand

```sh
agent slots history record <path> <slot>
```

Reads the slot's current live value and writes it to the history store immediately.
Use this for `on_demand` policy slots (see `HistoryConfig`) or ad-hoc snapshots.

**Arguments:**

| Argument | Required | Description |
|---|---|---|
| `path` | yes | Node path, e.g. `/station/sensor` |
| `slot` | yes | Slot name, e.g. `notes` |

**Examples:**

```sh
# Record the current value of the `notes` slot on /station/sensor
agent slots history record /station/sensor notes

# Same, JSON output
agent slots history record /station/sensor notes -o json
```

**Output (table):**
```
recorded (string)
```

**Output (JSON):**
```json
{
  "kind": "string",
  "recorded": true
}
```

**`kind`** values: `string`, `json` (never `number` or `bool` — use `slots telemetry record` for those).

**Error cases:**

| Condition | Response |
|---|---|
| Node not found | `404` with `{"error":"node not found"}` |
| Slot not declared on the node | `404` with `{"error":"slot ... not found on node ..."}` |
| Slot value is `null` | `422` with `{"error":"slot value is null — nothing to record"}` |
| Slot value is `Bool`/`Number` | Routes to telemetry store — use `slots telemetry record` instead |
| Agent started without `--db` | `503` with `{"error":"no history store configured ..."}` |

---

### Query history records

```sh
agent slots history list <path> <slot> [--from <ms>] [--to <ms>] [--limit <n>]
```

Returns history records for one slot in a time range, oldest first.

**Arguments:**

| Argument | Required | Description |
|---|---|---|
| `path` | yes | Node path |
| `slot` | yes | Slot name |
| `--from` | no | Start of range, Unix milliseconds (default: `0`) |
| `--to` | no | End of range, Unix milliseconds (default: now) |
| `--limit` | no | Max records to return (default: `1000`) |

**Examples:**

```sh
# All recorded history for /station/sensor notes (up to 1000 records)
agent slots history list /station/sensor notes

# Last 50 records
agent slots history list /station/sensor notes --limit 50

# Records in a 1-hour window
agent slots history list /station/sensor notes \
  --from 1700000000000 \
  --to   1700003600000

# JSON output with a small limit
agent slots history list /station/sensor notes --limit 3 -o json
```

**Output (JSON):**
```json
{
  "data": [
    {
      "id": 1,
      "node_id": "e1b3e4c7-c5e3-4a1d-821d-ae58548c5a2a",
      "slot_name": "notes",
      "slot_kind": "string",
      "ts_ms": 1776636922355,
      "value": "fault: none",
      "byte_size": 13,
      "ntp_synced": true,
      "last_sync_age_ms": null
    },
    {
      "id": 2,
      "node_id": "e1b3e4c7-c5e3-4a1d-821d-ae58548c5a2a",
      "slot_name": "notes",
      "slot_kind": "string",
      "ts_ms": 1776636922404,
      "value": "fault: overvoltage",
      "byte_size": 20,
      "ntp_synced": true,
      "last_sync_age_ms": null
    }
  ]
}
```

**Field reference:**

| Field | Type | Description |
|---|---|---|
| `id` | integer | Auto-assigned DB row ID |
| `node_id` | UUID string | Owning node's UUID |
| `slot_name` | string | Slot name |
| `slot_kind` | string | `string`, `json`, or `binary` |
| `ts_ms` | integer | Wall-clock Unix timestamp in milliseconds |
| `value` | any | Decoded value; `null` for Binary records |
| `byte_size` | integer | Stored byte size (used for quota accounting) |
| `ntp_synced` | boolean | Whether edge NTP was synced at record time |
| `last_sync_age_ms` | integer or null | Ms since last NTP sync; `null` if never synced |

**Getting a Unix ms timestamp:**
```sh
# Now
date +%s%3N

# One hour ago
echo $(($(date +%s%3N) - 3600000))
```

---

## `slots telemetry` — scalar history (Bool / Number)

### Record the current value on-demand

```sh
agent slots telemetry record <path> <slot>
```

Identical semantics to `slots history record` but routes to the time-series store
(`slot_timeseries` table). Use for `Bool` or `Number` slots.

**Examples:**

```sh
# Record the current temperature reading
agent slots telemetry record /station/ahu-1 temperature

# Record a boolean flag
agent slots telemetry record /station/door-1 open
```

**Output (JSON):**
```json
{
  "kind": "number",
  "recorded": true
}
```

**`kind`** values: `number` or `bool`.

---

### Query telemetry records

```sh
agent slots telemetry list <path> <slot> [--from <ms>] [--to <ms>] [--limit <n>]
```

Same flags as `slots history list`. Returns records from the scalar time-series store.

**Examples:**

```sh
# All temperature readings for /station/ahu-1 (up to 1000)
agent slots telemetry list /station/ahu-1 temperature

# Last 100 readings
agent slots telemetry list /station/ahu-1 temperature --limit 100

# 30-minute window
agent slots telemetry list /station/ahu-1 temperature \
  --from $(($(date +%s%3N) - 1800000))

# JSON output
agent slots telemetry list /station/ahu-1 temperature -o json
```

**Output (JSON):**
```json
{
  "data": [
    {
      "node_id": "e1b3e4c7-c5e3-4a1d-821d-ae58548c5a2a",
      "slot_name": "temperature",
      "ts_ms": 1776636950153,
      "value": 22.5,
      "ntp_synced": true,
      "last_sync_age_ms": null
    },
    {
      "node_id": "e1b3e4c7-c5e3-4a1d-821d-ae58548c5a2a",
      "slot_name": "temperature",
      "ts_ms": 1776636950182,
      "value": 23.1,
      "ntp_synced": true,
      "last_sync_age_ms": null
    }
  ]
}
```

For boolean slots, `value` is `true` or `false` (JSON boolean).
For number slots, `value` is a JSON number (integer or float).

---

## REST API equivalents

All CLI commands talk to these REST endpoints directly. Call them with `curl` or any HTTP client.

### `POST /api/v1/history/record`

On-demand record — handles both String/Json → history store and Bool/Number → telemetry store
based on the current live JSON value type.

```sh
curl -X POST http://localhost:8080/api/v1/history/record \
  -H 'Content-Type: application/json' \
  -d '{"path":"/station/sensor","slot":"notes"}'
# → {"recorded":true,"kind":"string"}

curl -X POST http://localhost:8080/api/v1/history/record \
  -H 'Content-Type: application/json' \
  -d '{"path":"/station/ahu-1","slot":"temperature"}'
# → {"recorded":true,"kind":"number"}
```

### `GET /api/v1/history`

Query structured history (String / Json / Binary slots).

```sh
# All records (up to default 1000)
curl 'http://localhost:8080/api/v1/history?path=/station/sensor&slot=notes'

# With time range and limit
curl 'http://localhost:8080/api/v1/history?path=/station/sensor&slot=notes&from=1700000000000&to=1700003600000&limit=50'
```

**Query parameters:** `path` (required), `slot` (required), `from` (Unix ms, default 0),
`to` (Unix ms, default now), `limit` (default 1000).

### `GET /api/v1/telemetry`

Query scalar history (Bool / Number slots). Same query parameters as `/api/v1/history`.

```sh
curl 'http://localhost:8080/api/v1/telemetry?path=/station/ahu-1&slot=temperature&limit=100'
```

---

## Practical workflows

### Snapshot current state of a device

```sh
# After writing new values, snapshot everything in one go
agent slots history record /station/ahu-1 notes
agent slots telemetry record /station/ahu-1 temperature
agent slots telemetry record /station/ahu-1 fan-speed
```

### Check what changed in the last hour

```sh
FROM=$(($(date +%s%3N) - 3600000))

# String fault codes
agent slots history list /station/ahu-1 fault-code \
  --from $FROM -o json | jq '.data[] | {ts_ms, value}'

# Numeric temperature
agent slots telemetry list /station/ahu-1 temperature \
  --from $FROM -o json | jq '.data[] | {ts_ms, value}'
```

### Consume history in a script

```sh
# Get timestamps and values as CSV-ish lines
agent slots telemetry list /station/ahu-1 temperature --limit 500 -o json \
  | jq -r '.data[] | "\(.ts_ms),\(.value)"'
```

---

## Known limitations (Stage 5/6)

- **On-demand only** — automatic recording (COV, Interval triggers) requires a `sys.core.history.config`
  child node and the running Historizer service (Stage 3, not wired to the agent yet).
  `record` commands bypass the Historizer and insert directly.
- **No RSQL filtering** — the query endpoints support `from`/`to`/`limit`, not full RSQL.
  Full RSQL (`?filter=value==...`) lands in Stage 5b.
- **Binary slot history** — insertion via REST body is not yet exposed (the DB table supports it);
  appears as `value: null` in responses.
- **Cross-schema merge** — a single query spanning both String and Number history of the same slot
  requires two separate calls (one to `/history`, one to `/telemetry`) merged client-side.
  This is by design (see SLOT-STORAGE.md § Non-goals).
