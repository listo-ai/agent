//! Wire shape for kind palette entries.
//!
//! Flattens [`spi::KindManifest`] and adds two derived fields (`org` and
//! `placement_class`) that the palette UI and RSQL surface both rely on.

use serde::Serialize;
use spi::KindManifest;

/// One row in the palette. `manifest` fields are flattened to the top
/// level so an RSQL filter like `id==sys.logic.function` or
/// `facets=contains=isCompute` hits the JSON shape directly.
#[derive(Debug, Clone, Serialize)]
pub struct KindDto {
    #[serde(flatten)]
    pub manifest: KindManifest,

    /// Derived: first two dot-segments of `manifest.id` joined with
    /// `.` — the publisher namespace. See [`derive_org`].
    pub org: String,

    /// `"free"` = empty `must_live_under`; `"bound"` = restricted
    /// parent. UI uses this to decide whether to show the kind in the
    /// global palette or only contextually under a matching parent.
    pub placement_class: &'static str,
}

impl KindDto {
    pub fn from_manifest(m: KindManifest) -> Self {
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
pub fn derive_org(kind_id: &str) -> String {
    let mut segs = kind_id.splitn(3, '.');
    match (segs.next(), segs.next()) {
        (Some(a), Some(b)) => format!("{a}.{b}"),
        (Some(a), None) => a.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spi::{
        Cardinality, CascadePolicy, ContainmentSchema, Facet, FacetSet, KindId, KindManifest,
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

    fn schema() -> ContainmentSchema {
        ContainmentSchema {
            must_live_under: vec![],
            may_contain: vec![],
            cardinality_per_parent: Cardinality::ManyPerParent,
            cascade: CascadePolicy::Strict,
        }
    }

    #[test]
    fn placement_class_free_when_no_parent_constraint() {
        let k = kind("sys.foo", FacetSet::default(), schema());
        assert_eq!(KindDto::from_manifest(k).placement_class, "free");
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
        assert_eq!(derive_org("foo.bar"), "foo.bar");
    }

    #[test]
    fn kind_dto_exposes_org_and_placement_class_in_json() {
        let dto = KindDto::from_manifest(kind(
            "com.listo.mqtt-client.client",
            FacetSet::of([Facet::IsCompute]),
            schema(),
        ));
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json.get("org").and_then(|v| v.as_str()), Some("com.listo"));
        assert_eq!(
            json.get("placement_class").and_then(|v| v.as_str()),
            Some("free"),
        );
    }
}
