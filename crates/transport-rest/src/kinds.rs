//! `/api/v1/kinds` — the palette endpoint.
//!
//! **Transport-layer module.** Per [`SKILLS/CODE-LAYOUT.md`](../../../../SKILLS/CODE-LAYOUT.md),
//! handlers here do exactly four things: extract inputs, call the
//! domain layer, translate to a DTO, return. The placement rule —
//! "can kind X live under parent Y?" — is domain logic and lives in
//! [`graph::placement_allowed`], called both from here and from
//! `graph::GraphStore::create_child`. One rule, one implementation.
//!
//! Returns every kind the [`KindRegistry`] currently holds, optionally
//! filtered by facet, by a parent path the caller is considering
//! placing the kind under, or by an [RSQL] filter expression over the
//! kind's exposed fields (see [`kinds_query_schema`]).
//!
//! Block-contributed kinds appear here the moment their block is
//! loaded — the endpoint is the palette source of truth for both
//! built-in and block kinds.
//!
//! # Query surface
//!
//! The endpoint accepts the standard pipeline params from
//! [`docs/design/QUERY-LANG.md`](../../docs/design/QUERY-LANG.md):
//!
//! | Param | Example |
//! |---|---|
//! | `facet` | `?facet=isCompute` — shortcut; composes with `filter` |
//! | `placeable_under` | `?placeable_under=/iot/broker` — admits only kinds the graph would accept under that parent |
//! | `filter` | `?filter=org==com.listo` — full RSQL over `id`/`org`/`display_name`/`facets`/`placement_class` |
//! | `sort` | `?sort=org,id` — comma-separated; `-field` for descending |
//!
//! ## The `org` field
//!
//! Derived, not on the manifest: first two dot-segments of the kind
//! id joined with `.`, so:
//! - `com.listo.mqtt-client.client` → `com.listo`
//! - `sys.logic.heartbeat`          → `sys.logic`
//! - `com.acme.hello.greeter`       → `com.acme`
//!
//! This matches the reverse-DNS convention block authors follow and
//! lets a palette UI group kinds by publisher. Finer granularity
//! (`com.listo.mqtt-client`) is reachable via `id=prefix=com.listo.mqtt-client.`.
//!
//! Response stays a bare `Vec<KindDto>` (not the generic `{data, meta}`
//! envelope) — the palette is always small and the CLI / Studio both
//! consume the flat array. If pagination ever matters here, add it as
//! an opt-in (`page`/`size` present → `Page<KindDto>`).

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use graph::placement_allowed;
use query::{FieldType, Operator, QueryRequest, QuerySchema, SortField};
use serde::{Deserialize, Serialize};
use spi::{Facet, KindManifest};

use crate::routes::{parse_path, ApiError};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/kinds", get(list_kinds))
}

#[derive(Debug, Deserialize)]
pub struct ListKindsQuery {
    /// Filter to kinds carrying this facet (camelCase: `isProtocol`).
    /// Kept as a dedicated param — simpler than the RSQL
    /// `facets=contains=<facet>` for the common single-facet case,
    /// and composes with `filter` (both must match).
    #[serde(default)]
    pub facet: Option<Facet>,

    /// Filter to kinds the graph would accept as a child of this
    /// existing parent path. Combines the parent's `may_contain` rules
    /// with the candidate's `must_live_under` rules — same predicate
    /// `GraphStore::create_child` uses.
    #[serde(default)]
    pub placeable_under: Option<String>,

    /// RSQL filter expression. See [`kinds_query_schema`] for the
    /// exposed fields and operator allow-list.
    #[serde(default)]
    pub filter: Option<String>,

    /// Comma-separated sort fields (`-field` for descending). Falls
    /// back to ascending `id` when absent.
    #[serde(default)]
    pub sort: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KindDto {
    #[serde(flatten)]
    pub manifest: KindManifest,

    /// Derived: first two dot-segments of `manifest.id` joined with
    /// `.` — the publisher namespace. See module docs.
    pub org: String,

    /// `free` = empty `must_live_under`; `bound` = restricted parent.
    /// UI uses this to decide whether to show the kind in the global
    /// palette or only contextually under a matching parent.
    pub placement_class: &'static str,
}

impl KindDto {
    fn from_manifest(m: KindManifest) -> Self {
        let placement_class = if m.containment.must_live_under.is_empty() {
            "free"
        } else {
            "bound"
        };
        let org = derive_org(m.id.as_str());
        Self {
            manifest: m,
            org,
            placement_class,
        }
    }
}

/// Extract the publisher-org prefix from a kind id. First two
/// dot-segments joined; single-segment ids round-trip as-is.
///
/// Contract:
/// - `"com.listo.mqtt-client.client"` → `"com.listo"`
/// - `"sys.logic.heartbeat"`          → `"sys.logic"`
/// - `"sys"`                          → `"sys"`
/// - `""`                             → `""`
fn derive_org(kind_id: &str) -> String {
    let mut segs = kind_id.splitn(3, '.');
    match (segs.next(), segs.next()) {
        (Some(a), Some(b)) => format!("{a}.{b}"),
        (Some(a), None) => a.to_string(),
        _ => String::new(),
    }
}

/// Queryable schema for the palette endpoint.
///
/// Exposed fields (all surface on the emitted JSON, so the framework's
/// JSON-walking executor can filter / sort on them directly):
///
/// | Field | Type | Ops | Example |
/// |---|---|---|---|
/// | `id`              | Text    | eq, ne, prefix, in | `id==sys.logic.function` |
/// | `org`             | Text    | eq, ne, prefix, in | `org==com.listo` |
/// | `display_name`    | Text    | eq, ne, prefix     | `display_name=prefix=MQTT` |
/// | `facets`          | TextArr | contains, in       | `facets=contains=isCompute` |
/// | `placement_class` | Text    | eq, ne             | `placement_class==free` |
///
/// Default sort: ascending `id` (alphabetical, stable). Page size is
/// unused here — we never paginate the palette — but the schema
/// requires a value, so we set it well above any realistic kind count.
pub(crate) fn kinds_query_schema() -> QuerySchema {
    QuerySchema::new(10_000, 10_000)
        .field(
            "id",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix, Operator::In],
        )
        .field(
            "org",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix, Operator::In],
        )
        .field(
            "display_name",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix],
        )
        .field(
            "facets",
            FieldType::TextArr,
            [Operator::Contains, Operator::In],
        )
        .field(
            "placement_class",
            FieldType::Text,
            [Operator::Eq, Operator::Ne],
        )
        .default_sort([SortField::asc("id")])
}

async fn list_kinds(
    State(s): State<AppState>,
    Query(q): Query<ListKindsQuery>,
) -> Result<Json<Vec<KindDto>>, ApiError> {
    let registry = s.graph.kinds();
    let mut all = registry.all();

    // Concrete-param shortcuts run first — cheaper than JSON-walking
    // every manifest and also the only way to check `placeable_under`,
    // which needs the graph (not just the manifest).
    if let Some(f) = q.facet {
        all.retain(|m| m.facets.contains(f));
    }

    if let Some(parent_raw) = q.placeable_under.as_deref() {
        let parent_path = parse_path(parent_raw)?;
        let parent = s
            .graph
            .get(&parent_path)
            .ok_or_else(|| ApiError::not_found(format!("no node at `{parent_path}`")))?;
        let parent_manifest = registry
            .get(&parent.kind)
            .ok_or_else(|| ApiError::bad_request("parent kind is not registered"))?;
        all.retain(|candidate| placement_allowed(&parent.kind, &parent_manifest, candidate));
    }

    // Materialise DTOs *before* the RSQL step so the derived `org`
    // field is part of the JSON shape the executor walks.
    let dtos: Vec<KindDto> = all.into_iter().map(KindDto::from_manifest).collect();

    // If the caller didn't use the query framework, the old contract
    // holds: ascending by id, no pagination, flat `Vec`.
    if q.filter.is_none() && q.sort.is_none() {
        let mut dtos = dtos;
        dtos.sort_by(|a, b| a.manifest.id.as_str().cmp(b.manifest.id.as_str()));
        return Ok(Json(dtos));
    }

    // Full pipeline. Palette is bounded, so we always return every
    // matching row — `size` is pinned high in `kinds_query_schema`
    // so `execute` never truncates.
    let query = query::validate(
        &kinds_query_schema(),
        QueryRequest {
            filter: q.filter,
            sort: q.sort,
            page: Some(1),
            size: Some(kinds_query_schema().max_page_size()),
        },
    )
    .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let page = query::execute(dtos, &query).map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(Json(page.data))
}

#[cfg(test)]
mod tests {
    //! Transport-layer tests only.
    //!
    //! The containment rule (`placement_allowed`) lives in `graph` and
    //! is tested there — duplicating those tests here would drift. We
    //! test what this module actually owns: DTO shaping (including the
    //! derived `org` field) and the RSQL query-schema wiring.
    use super::*;
    use spi::{
        Cardinality, CascadePolicy, ContainmentSchema, FacetSet, KindId, KindManifest,
        ParentMatcher,
    };

    fn kind(id: &str, facets: FacetSet, containment: ContainmentSchema) -> KindManifest {
        KindManifest {
            id: KindId::new(id),
            display_name: None,
            facets,
            containment,
            slots: Vec::new(),
            settings_schema: serde_json::Value::Null,
            msg_overrides: Default::default(),
            trigger_policy: Default::default(),
            schema_version: 1,
            views: Vec::new(),
        }
    }

    fn schema(must: Vec<ParentMatcher>, may: Vec<ParentMatcher>) -> ContainmentSchema {
        ContainmentSchema {
            must_live_under: must,
            may_contain: may,
            cardinality_per_parent: Cardinality::ManyPerParent,
            cascade: CascadePolicy::Strict,
        }
    }

    #[test]
    fn placement_class_free_when_no_parent_constraint() {
        let k = kind("sys.foo", FacetSet::default(), schema(vec![], vec![]));
        assert_eq!(KindDto::from_manifest(k).placement_class, "free");
    }

    #[test]
    fn placement_class_bound_when_must_live_under_present() {
        let k = kind(
            "sys.foo.point",
            FacetSet::default(),
            schema(vec![ParentMatcher::Kind(KindId::new("sys.foo"))], vec![]),
        );
        assert_eq!(KindDto::from_manifest(k).placement_class, "bound");
    }

    #[test]
    fn derive_org_extracts_first_two_segments() {
        assert_eq!(derive_org("com.listo.mqtt-client.client"), "com.listo");
        assert_eq!(derive_org("sys.logic.heartbeat"), "sys.logic");
        assert_eq!(derive_org("com.acme.hello.greeter"), "com.acme");
    }

    #[test]
    fn derive_org_single_segment_round_trips() {
        assert_eq!(derive_org("sys"), "sys");
        assert_eq!(derive_org(""), "");
    }

    #[test]
    fn derive_org_two_segments_uses_both() {
        // `foo.bar` → "foo.bar" (no third segment to split off).
        assert_eq!(derive_org("foo.bar"), "foo.bar");
    }

    #[test]
    fn kind_dto_exposes_org_in_json() {
        let dto = KindDto::from_manifest(kind(
            "com.listo.mqtt-client.client",
            FacetSet::of([Facet::IsCompute]),
            schema(vec![], vec![]),
        ));
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json.get("org").and_then(|v| v.as_str()), Some("com.listo"));
        assert_eq!(
            json.get("placement_class").and_then(|v| v.as_str()),
            Some("free"),
        );
    }

    /// End-to-end: the query schema + executor filters by the derived
    /// `org` field, which is the user-facing feature this whole change
    /// ships.
    #[test]
    fn filter_by_org_narrows_to_publisher() {
        use query::{validate, QueryRequest};

        let items = vec![
            KindDto::from_manifest(kind(
                "com.listo.mqtt-client.client",
                FacetSet::default(),
                schema(vec![], vec![]),
            )),
            KindDto::from_manifest(kind(
                "com.listo.mqtt-client.pub",
                FacetSet::default(),
                schema(vec![], vec![]),
            )),
            KindDto::from_manifest(kind(
                "com.acme.hello.greeter",
                FacetSet::default(),
                schema(vec![], vec![]),
            )),
            KindDto::from_manifest(kind(
                "sys.logic.heartbeat",
                FacetSet::default(),
                schema(vec![], vec![]),
            )),
        ];

        let q = validate(
            &kinds_query_schema(),
            QueryRequest {
                filter: Some("org==com.listo".into()),
                sort: None,
                page: Some(1),
                size: Some(100),
            },
        )
        .unwrap();

        let page = query::execute(items, &q).unwrap();
        assert_eq!(page.meta.total, 2);
        let ids: Vec<&str> = page.data.iter().map(|d| d.manifest.id.as_str()).collect();
        assert!(ids.contains(&"com.listo.mqtt-client.client"));
        assert!(ids.contains(&"com.listo.mqtt-client.pub"));
    }
}
