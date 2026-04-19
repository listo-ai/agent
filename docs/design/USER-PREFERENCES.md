# User Preferences

## What it is

Per-user (and per-org) preferences for locale, timezone, units, date/time format, language, and a handful of UI bits. Applied at the API/presentation edge, never in storage.

The goal is to decide **now** — while the API surface is small — how we canonicalise values in the database and convert them at the edge, so we don't ship a US-centric API and spend the next two years retrofitting i18n.

## The core idea

**Store canonical. Convert at the edge.**

- Timestamps: UTC in the database, always. No local-time columns, ever.
- Measurements: stored in a single canonical unit per field (SI/metric). Converted to the user's preferred unit on read.
- Strings shown to humans: stored as message keys + parameters, translated at the edge via the user's locale.
- Money (if/when): minor units + ISO 4217 code.

One source of truth, N presentations. A telemetry row written by a US edge agent and read by an Australian operator goes through zero ambiguity.

## Scope

In scope:

- Data model for user and org preferences.
- Conventions for canonical storage (time, units, numbers, money).
- Edge-of-API conversion points (REST responses, CLI output, Studio UI).
- Language / i18n strategy for UI strings, error messages, and notifications.
- Library choices — we don't reinvent tz, locale, or unit handling.

Out of scope (for now):

- Translating user-authored content (flow names, node labels). That's a separate problem (see *Future*).
- RTL layout and per-locale UI polish — handled by the Studio team once i18n framework is in.
- Currency conversion / FX rates — we only store and display, we don't convert values.

## Preferences model

> **Display-only preferences** (fields with no canonical storage counterpart, such as `week_start`, `theme`, `time_format`, `date_format`, `number_format`) participate in the three-layer resolution model but are **never involved in data conversion**. They affect only how values are rendered to a human. The "store canonical, convert at the edge" principle applies only to physical quantities (time, units, money). Display-only prefs are listed here alongside quantity prefs for completeness — they do not feed into the `UnitRegistry` or the serialisation middleware.

Three layers, resolved inside-out: **user-per-org** overrides **org** overrides **system default**. Users can belong to multiple orgs; preferences are scoped to the active org in the session so that switching orgs switches the entire preference context.

```
org_preferences
  org_id (PK)
  timezone            TEXT      -- IANA, e.g. "Australia/Brisbane"
  locale              TEXT      -- BCP-47, e.g. "en-AU"
  language            TEXT      -- BCP-47 language subtag, e.g. "en", "zh", "es"
  unit_system         TEXT      -- "metric" | "imperial"
  temperature_unit    TEXT      -- "C" | "F" | "auto"
  pressure_unit       TEXT      -- "kPa" | "psi" | "bar" | "auto"
  date_format         TEXT      -- "auto" | "YYYY-MM-DD" | "DD/MM/YYYY" | "MM/DD/YYYY"
  time_format         TEXT      -- "auto" | "24h" | "12h"
  week_start          TEXT      -- "auto" | "monday" | "sunday"
  number_format       TEXT      -- "auto" | "1,234.56" | "1.234,56" | "1 234,56"
  currency            TEXT      -- ISO 4217 or "auto" (derive from locale); never NULL in resolved view
  updated_at          INTEGER   -- UTC epoch ms

user_preferences
  user_id   (PK, FK)
  org_id    (PK, FK)            -- scope: a user has one prefs row per org they belong to
  -- Same columns as org_preferences, all nullable.
  -- NULL means "inherit from org".
  language            TEXT      -- nullable; inherits org.language if NULL
  theme               TEXT      -- "light" | "dark" | "system"  (user-only, no org fallback)
  updated_at          INTEGER   -- UTC epoch ms
```

Resolution: `user_value ?? org_value ?? system_default`. `"auto"` means *derive from `locale`*; explicit values override. The hard-coded system default locale is `en-US` — if neither user nor org has a locale and the field resolves to `auto`, the formatter uses `en-US`. This fallback is explicit, not implicit: `GET /v1/me/preferences` returns the resolved value so clients never see `NULL` or `auto` in the final view.

`currency` semantics: `"auto"` → derive from `locale` (e.g. `en-AU` → `AUD`). Explicit ISO 4217 overrides. The resolved view always returns a concrete code so display code never has to decide what `NULL` means.

All timestamps (`updated_at` and every other `INTEGER`-typed time column in the system) are UTC epoch **milliseconds**. See the Time section below.

### Why per-unit overrides, not just `unit_system`

A flat metric/imperial flag fails real users immediately: Australians want metric-everything-except-°C-but-display-°F-on-the-BBQ-thermometer; UK users want metric weather and imperial road signs. Fields like `temperature_unit` and `pressure_unit` let us cover the 95% case without exploding the schema. Default them to `"auto"` (derive from `unit_system`) and only fill in on explicit override.

## Canonical storage

### Time

- All `TIMESTAMP` columns store UTC epoch milliseconds (`INTEGER`) or ISO-8601 with `Z`.
- Never store local time. Never store a TZ offset alongside a timestamp — the TZ belongs on the user/event, not the column.
- Durations are stored in base units (ms, or s for coarse fields) as integers.

### Units

A per-field **unit registry** declares the canonical unit for every quantity the system knows about:

```
temperature      → °C
pressure         → kPa
flow_rate        → L/s
volume           → L
mass             → kg
length           → m
energy           → kWh
power            → W
speed            → m/s
ratio            → 0.0–1.0 (not 0–100; use quantity: Ratio in slot schema)
```

- Telemetry writers convert to canonical at ingest (the SPI slot schema already knows the field's unit).
- Read path: the presentation layer looks up the user's preferred unit for that quantity and converts.
- New sensor types add a row to the registry — they don't invent their own storage convention.

This is more robust than a single `unit_system` flag because it survives us adding domains we haven't thought of yet (air quality, acoustic, electrical).

### Money

Minor units (integer) + ISO 4217 currency code column. Never float. No implicit currency — every money value carries its code.

## Conversion points

Conversion happens at **exactly one** layer per surface:

| Surface | Converts where |
|---|---|
| REST API | Response serialisation middleware, keyed off the caller's token claims |
| CLI | Client-side, using preferences fetched once per session |
| Studio UI | Client-side formatter, preferences loaded with session |
| Logs / audit trail | **Never converted** — always canonical UTC + SI |
| Inter-service RPC | **Never converted** — canonical only |

Rule of thumb: if two services talk to each other, they use canonical. Only the human-facing edge formats.

## Language (i18n)

Three separate things, often conflated:

1. **Locale** (`en-AU`, `zh-CN`, `es-MX`) — drives number/date/currency formatting. Always set.
2. **Language** (`en`, `zh`, `es`) — drives which translation bundle the UI loads. Derived from locale unless explicitly overridden.
3. **Content language** — the language user-authored strings (flow names, node descriptions) were written in. Separate problem, not solved here.

### Strategy

- UI strings live as **message keys** in the codebase (`flows.create.button`, not `"Create Flow"`).
- Translation bundles per language (`en.json`, `zh.json`, `es.json`, …) shipped with Studio.
- Backend-originated messages that a human will see (validation errors, notifications) also use message keys. The backend returns `{ code: "flow.invalid_cycle", params: { node: "x" } }` — the client translates. This keeps the backend language-neutral and means adding a language doesn't require a backend deploy.
- Initial target languages: English (`en`) as source, then prioritised by actual customer demand. Don't pre-translate to ten languages before anyone asks — stale translations are worse than English fallback.
- Fallback chain: requested language → language family (e.g. `zh-TW` → `zh`) → `en`. Missing keys fall through, never error.

### Why not translate server-side

- Backend stays stateless w.r.t. presentation — one response shape regardless of caller.
- Translation updates ship with the client, no backend deploy.
- LLM/MCP consumers can choose to not translate at all and get stable machine-readable codes.

## Library choices

We use existing crates for everything non-trivial. No bespoke tz math, no hand-rolled unit tables.

| Concern | Crate | Notes |
|---|---|---|
| Timezone-aware datetime | [`jiff`](https://crates.io/crates/jiff) | Modern replacement for chrono/time. IANA tz built in, sane API, Rust 2024 idioms. Preferred. |
| IANA tz database | bundled with `jiff` | No separate `tzdata` crate needed. |
| Locale parsing / matching | [`icu_locale`](https://crates.io/crates/icu_locale) (ICU4X) | BCP-47 parsing, fallback chains. |
| Number / date formatting | [`icu_datetime`](https://crates.io/crates/icu_datetime), [`icu_decimal`](https://crates.io/crates/icu_decimal) (ICU4X) | Locale-aware formatters, no_std friendly. |
| Unit conversion | [`uom`](https://crates.io/crates/uom) | Type-safe SI units. Compile-time dimensional analysis. |
| Translation bundles | [`fluent`](https://crates.io/crates/fluent) (+ `fluent-bundle`) | Mozilla's Fluent — handles plurals, gender, placeholders. Clients load `.ftl` files. |
| ISO 4217 currency codes | [`iso_currency`](https://crates.io/crates/iso_currency) | Static table, no FX logic. |

Principle: **ICU4X for presentation, `jiff` for time, `uom` for units, Fluent for translations.** Don't mix, don't wrap unnecessarily, don't write our own.

## Slot units

Units live on the **slot schema**, not on the user. The user preference only decides how a slot's value is *rendered*; the slot itself declares what physical quantity it represents and (if the sensor natively emits something non-canonical) what unit it's stored in.

Three optional fields are added to `SlotSchema` (in [`crates/spi/src/slot_schema.rs`](../../crates/spi/src/slot_schema.rs)):

```rust
pub struct SlotSchema {
    // ...existing...

    /// Physical quantity this slot measures (e.g. "temperature",
    /// "pressure", "flow_rate"). Looked up in the UnitRegistry to
    /// find the canonical storage unit. `None` = dimensionless.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<Quantity>,

    /// Unit the sensor natively emits. The ingest pipeline converts
    /// from this unit to `quantity`'s canonical unit before storage.
    /// If absent, the sensor is assumed to already emit the
    /// canonical unit (no ingest-time conversion is applied).
    /// Must be listed in `UnitRegistry::quantity(q).allowed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensor_unit: Option<Unit>,

    /// Unit the **stored** value is expressed in. Defaults to the
    /// quantity's canonical unit (i.e. absent = canonical).
    /// Set only when ingest-time conversion is explicitly opted out
    /// (see rules below). The read path uses this as the source
    /// unit for display conversion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<Unit>,
}
```

Rules:

- Only meaningful for `value_kind: Number` (and occasionally `Bool` for thresholded values). Ignored on `Json`/`String`/`Binary`.
- `quantity` is nullable because many slots are genuinely dimensionless (counts, IDs, enums, already-normalised ratios).
- Authors **pick from the registry**; they cannot invent a quantity inline. New quantities land via platform PR so the conversion tables and UI labels stay consistent.
- `SlotSchema::sensor_unit` tells the ingest pipeline what unit the raw sensor value is in. The pipeline converts to canonical before writing. If absent, no conversion is applied (the sensor is expected to already emit the canonical unit).
- `SlotSchema::unit` **describes the stored value's unit**, not the sensor's native unit. Default behaviour: ingest converts sensor output to the quantity's canonical unit, and `SlotSchema::unit` is either absent or equals that canonical unit. The field exists so that specific slots can *opt out* of ingest-time conversion and store values in a non-canonical unit (the historian then records that unit, and read-path conversion uses it as the source). This is rare and discouraged — use it only when ingest-time conversion is too lossy or too expensive (e.g. a high-rate raw-counts sensor where the calibration factor isn't known at write time).

### `UnitRegistry`

Lives in `crates/spi/src/units.rs`. Static at build time, extended by the platform (not by extensions).

```rust
use uom::si::f64 as si;

/// A physical quantity we know how to store and render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quantity {
    Temperature,
    Pressure,
    FlowRate,
    Volume,
    Mass,
    Length,
    Energy,
    Power,
    Speed,
    /// Dimensionless 0.0–1.0. Canonical unit is `Unit::Ratio` (0–1).
    /// `Unit::Percent` (0–100) is a display unit only — never a
    /// storage unit. The registry rejects a slot declaring
    /// `quantity: Ratio, unit: Percent`.
    Ratio,
    Duration,
    // ... extended as platform needs grow
}

/// A concrete unit. Closed enum so the wire format is stable and
/// the UI knows every label. Extensions cannot add variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    // temperature
    Celsius, Fahrenheit, Kelvin,
    // pressure
    Kilopascal, Bar, Psi, Hectopascal,
    // flow rate
    LitersPerSecond, LitersPerMinute, CubicMetersPerHour, GallonsPerMinute,
    // volume
    Liter, CubicMeter, UsGallon, ImperialGallon,
    // mass
    Kilogram, Gram, Pound, Ounce,
    // length
    Meter, Millimeter, Kilometer, Inch, Foot, Mile,
    // energy / power
    Kilowatt, Watt, Horsepower, KilowattHour, Joule,
    // speed
    MetersPerSecond, KilometersPerHour, MilesPerHour, Knot,
    // dimensionless
    Ratio, Percent,
    // duration
    Millisecond, Second, Minute, Hour,
}

pub struct QuantityDef {
    pub canonical: Unit,
    pub allowed: &'static [Unit],
    /// Short symbol for rendering when the user has no explicit
    /// preference (e.g. "°C"). Locale-specific rendering goes
    /// through ICU4X on the client.
    pub symbol: &'static str,
}

pub trait UnitRegistry: Send + Sync {
    fn quantity(&self, q: Quantity) -> &'static QuantityDef;

    /// Convert a value from `from` to `to`. Both must be listed in
    /// `quantity(q).allowed`. Backed by `uom` so the math is
    /// dimension-checked at compile time internally.
    fn convert(&self, q: Quantity, v: f64, from: Unit, to: Unit) -> f64;
}

/// Production registry — built from the static quantity table.
pub struct StaticRegistry;

/// Default platform registry. Code paths take `&dyn UnitRegistry`
/// so tests can inject a fixture registry (see `TestRegistry` in
/// the crate's test module) without touching a global.
pub fn default_registry() -> &'static dyn UnitRegistry {
    &StaticRegistry
}
```

Testability note: the registry is a trait object passed through context (typically via `Arc<dyn UnitRegistry>` in services, `&'static dyn UnitRegistry` in the hot path). A `TestRegistry` struct swaps in custom quantity tables so unit-conversion tests don't depend on the global static.

Conversion is delegated to [`uom`](https://crates.io/crates/uom) internally — we never hand-write conversion factors. The registry is the thin serialisable veneer over `uom`'s typed system, because `uom`'s types don't serialise cleanly across the wire.

### Enum versioning and canonical-unit migration

The closed `Quantity`/`Unit` enums are part of the wire format and must be treated as a public API. Rules:

- **Adding** a new `Quantity` or `Unit` variant is always backward-compatible (old clients ignore unknown values).
- **Renaming or removing** a variant requires a major platform version bump. The old name must be kept as a deprecated alias for at least one major version.
- **Changing a quantity's canonical unit** (e.g. deciding after the fact to store `Ratio` as 0–100 instead of 0–1) requires a migration plan: (a) bump the major platform version, (b) provide a one-off backfill script for existing telemetry rows, (c) update the registry and all ingest/read paths atomically. This is expected to be rare and high-cost — choose canonical units carefully up front.
- `GET /v1/units` returns an `X-Platform-Version` header alongside the ETag so clients can detect a registry change on reconnect.

### Extension-defined quantities

The closed enum is deliberate: the wire format must be stable and the UI needs to know every label. Extensions that need a quantity we don't ship cannot add an enum variant; they go through the platform PR process:

1. Extension author files an issue describing the quantity, canonical unit, allowed display units, and symbol.
2. Platform PR adds the variant, canonical mapping, and `uom` wiring. UI gets a new label bundle entry.
3. Ships on the next platform release. Extension's manifest can then reference the new `quantity`.

Friction is intentional. A quantity is part of the public data model; letting extensions invent new ones would fragment telemetry across tenants and break cross-extension dashboards. Extensions that can't wait can store the value as dimensionless (`quantity: None`) and do their own formatting — with the tradeoff that users can't set a unit preference for it.

### How a read works end-to-end

1. Agent writes telemetry: `slot.set(72.4)`. Sensor is natively °F; slot schema declares `quantity: Temperature, sensor_unit: Fahrenheit` and omits `unit` (the default: store in the canonical unit for the quantity). Ingest converts `72.4 °F` → `22.44 °C`, stores `22.44`.
2. REST handler queries telemetry. It returns a column-oriented response so unit metadata is declared once per series, not per row (see Response shape below).
3. Serialisation middleware resolves the caller's `temperature_unit` pref (say, `"F"`), calls `REGISTRY.convert(Temperature, 22.44, Celsius, Fahrenheit)` → `72.4`, and sets the series' `unit` to `"fahrenheit"`.
4. Studio renders `72.4 °F`.

MCP/LLM consumers send `Accept-Units: canonical` and skip step 3 — they get `22.44` with `"unit": "celsius"` and a machine-stable quantity code.

## API surface

- `GET /v1/me/preferences?org=<org_id>` — resolved (user-per-org ∪ org ∪ defaults) view for the given org. `org` defaults to the active org in the session.
- `PATCH /v1/me/preferences?org=<org_id>` — user layer for that org only. Fields set to `null` revert to inherit.
- `GET /v1/orgs/{id}/preferences` — org layer, admin only.
- `PATCH /v1/orgs/{id}/preferences` — admin only.
- `GET /v1/units` — the public quantity/unit registry, so clients can render labels and offer unit-picker UIs without hard-coding. Returns an ETag; versioned alongside the platform release.

### Content negotiation

Unit conversion is selected via a custom `Accept-Units` request header:

```
Accept-Units: preferred    # default — user's display units applied
Accept-Units: canonical    # MCP / programmatic — SI values, stable quantity codes
```

Responses set `Vary: Accept-Units` so caches key on the selected mode. `Content-Language` reports the language actually used so clients can detect fallback.

> **Why a custom header, not an `Accept` media-type parameter?** Most CDNs (CloudFront, Fastly, nginx default config) correctly vary on explicit header names but collapse or strip `Accept` media-type parameters, effectively treating `application/json; units=canonical` and `application/json; units=preferred` as the same cache key. A custom header is more reliably varied on in practice.

### Response shape

Unit and quantity metadata are declared **per series / per column**, not per value, to avoid the per-row payload multiplier on time-series responses. Timeseries:

```json
{
  "series": [{
    "slot": "temp_in",
    "quantity": "temperature",
    "unit": "fahrenheit",
    "points": [[1713456000000, 72.4], [1713456060000, 72.6], ...]
  }]
}
```

Single-value reads (a single slot, not a timeseries) use the inline form `{ "value": 72.4, "unit": "fahrenheit", "quantity": "temperature" }` since there's nothing to hoist. The rule: **unit/quantity are declared once at the tightest scope that covers homogeneous values.**

## Decisions (previously open)

**Preferences delivery: hybrid, claims for stable fields only.**
Embed `timezone`, `locale`, `language` in the JWT (they change rarely and every response needs them). Everything else (`unit_system`, per-unit overrides, formats, theme) is fetched once per session via `GET /v1/me/preferences` and cached client-side with an ETag. Mutations invalidate the cache. This avoids the stale-JWT problem for the volatile fields while keeping the hot path (every telemetry read) free of an extra fetch.

JWT TTL is **15 minutes** with silent refresh. When a user mutates any of the JWT-embedded prefs (`timezone`, `locale`, `language`), the server issues a fresh token immediately as part of the `PATCH` response (`Set-Cookie` / response body depending on the auth transport). The client must replace its current token before the next request. On conflict (stale JWT claim vs. fetched session pref), **the fetched session pref wins** — clients should always populate the preference context from `GET /v1/me/preferences`, treating the JWT claims only as a fast-path hint for the server side (e.g. email rendering, audit exports) where no session fetch is possible.

**Per-device timezone: client-side only.**
Studio reads the OS timezone and may override the profile TZ for display purposes, persisted in local storage. The server never sees this — the user's profile TZ stays authoritative for anything the server renders (emails, notifications, audit exports). Rationale: a traveller on a laptop shouldn't have their scheduled reports shift, but their live dashboards should reflect local time. Splitting at the client is the clean seam.

**MCP / programmatic raw mode: `Accept-Units: canonical`.**
Default is `units=preferred` (conversion applied). MCP clients and CLI scripts send `Accept-Units: canonical` to get SI values and stable quantity codes. Custom header rather than an `Accept` media-type parameter so CDNs and reverse proxies vary on it correctly (see Content negotiation above). No separate endpoints, no URL variants.

## Rollout

This lands incrementally, not as a big-bang greenfield change.

- **Phase 0** — land the conventions. Timestamps already UTC epoch-ms (verify and document). Document `Quantity`/`Unit` enums in `spi`. No behavioural change.
- **Phase 1** — schema + API. Add `org_preferences` / `user_preferences` tables, `GET`/`PATCH` endpoints, JWT claim extension for `timezone`/`locale`/`language`. Clients that don't know about preferences keep working (server returns current US defaults, which is the status quo).
- **Phase 2** — slot-level units. Add `quantity`/`unit` to `SlotSchema`. Existing slots default to `None` → no conversion, no behaviour change. Extension authors can start annotating.
- **Phase 3** — conversion middleware. REST serialisation applies unit prefs when both (a) the slot declares a quantity and (b) the caller's pref differs from canonical. Add `Accept: ...; units=canonical` support.
- **Phase 4** — i18n. Wire Fluent into Studio; move backend error messages to message codes; add first non-English bundle on demand.

Each phase is independently deployable and reversible. No client is forced to upgrade to keep working.

## Future

- **User-authored content translation** — translations sidecar table keyed by `(entity_id, lang)`, authored language as canonical. Surface a "translate this" affordance in Studio once multi-language customers exist.
- **Per-org translation overrides** — customer wants "Site" instead of "Location" in their UI. Solvable via custom Fluent bundles loaded after the default, keyed on org.
- **Extension-defined quantities** — documented path above (platform PR). If demand gets high, consider a versioned "quantity catalog" extension point, but not before we have concrete asks.
- **RTL layout and per-locale UI polish** — Studio team handoff once the i18n framework lands.
- **Per-device timezone sync protocol** — currently client-local. If users report confusion about which TZ is active, add a session-scoped `X-User-Timezone` request header so the server can render server-side emails/exports in the same TZ Studio is displaying.
- **Accessibility preferences** (reduced motion, high contrast, font scale) — extend `user_preferences` with the same inheritance model.
- **Currency FX conversion** — out of scope deliberately; revisit only if we ever aggregate money across currencies.
