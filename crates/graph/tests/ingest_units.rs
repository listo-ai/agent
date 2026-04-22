#![allow(clippy::unwrap_used, clippy::panic)]
//! Ingest-time unit normalisation end-to-end through
//! `GraphStore::write_slot`.
//!
//! The conversion helper + rule table live in `spi::units::normalize_for_storage`
//! (and are unit-tested there). This test proves the graph actually
//! calls it — i.e. a write of `72.4` to a slot declared
//! `quantity: Temperature, sensor_unit: Fahrenheit` lands in memory as
//! ~22.444, not 72.4.

use std::sync::Arc;

use graph::{GraphStore, KindRegistry, NullSink};
use serde_json::json;
use spi::{
    Cardinality, CascadePolicy, ContainmentSchema, Facet, FacetSet, KindId, KindManifest, NodePath,
    Quantity, SlotRole, SlotSchema, SlotValueKind, Unit,
};

fn temp_kind() -> KindManifest {
    KindManifest {
        id: KindId::new("test.sensor"),
        display_name: None,
        // `IsAnywhere` bypasses the parent-kind placement check so the
        // fixture doesn't need to teach sys.core.station about
        // `test.sensor`.
        facets: FacetSet::of([Facet::IsAnywhere]),
        containment: ContainmentSchema {
            must_live_under: vec![],
            may_contain: vec![],
            cardinality_per_parent: Cardinality::ManyPerParent,
            cascade: CascadePolicy::Strict,
        },
        slots: vec![
            // Sensor-unit is Fahrenheit; canonical (Celsius) is the
            // implicit target. Ingest must convert.
            SlotSchema::new("temp_f_sensor", SlotRole::Input)
                .with_kind(SlotValueKind::Number)
                .writable()
                .with_quantity(Quantity::Temperature)
                .with_sensor_unit(Unit::Fahrenheit),
            // Already-canonical sensor: no conversion.
            SlotSchema::new("temp_c_sensor", SlotRole::Input)
                .with_kind(SlotValueKind::Number)
                .writable()
                .with_quantity(Quantity::Temperature)
                .with_sensor_unit(Unit::Celsius),
            // No quantity → passthrough even for Number.
            SlotSchema::new("raw", SlotRole::Input)
                .with_kind(SlotValueKind::Number)
                .writable(),
        ],
        settings_schema: serde_json::Value::Null,
        msg_overrides: Default::default(),
        trigger_policy: Default::default(),
        schema_version: 1,
        views: Vec::new(),
    }
}

fn fresh_store() -> Arc<GraphStore> {
    let kinds = KindRegistry::new();
    graph::seed::register_builtins(&kinds);
    kinds.register(temp_kind());
    let store = Arc::new(GraphStore::new(kinds, Arc::new(NullSink)));
    store.create_root(KindId::new("sys.core.station")).unwrap();
    store
        .create_child(&NodePath::root(), KindId::new("test.sensor"), "s")
        .unwrap();
    store
}

fn read_slot(store: &GraphStore, slot: &str) -> f64 {
    let snap = store.get(&NodePath::root().child("s")).unwrap();
    snap.slot_values
        .iter()
        .find(|(n, _)| n == slot)
        .unwrap()
        .1
        .value
        .as_f64()
        .unwrap()
}

#[test]
fn fahrenheit_sensor_is_stored_as_celsius() {
    let store = fresh_store();
    let path = NodePath::root().child("s");
    store.write_slot(&path, "temp_f_sensor", json!(72.4)).unwrap();
    let stored = read_slot(&store, "temp_f_sensor");
    assert!(
        (stored - 22.444).abs() < 0.01,
        "expected ~22.44 °C in storage, got {stored}"
    );
}

#[test]
fn celsius_sensor_is_stored_unchanged() {
    let store = fresh_store();
    let path = NodePath::root().child("s");
    store.write_slot(&path, "temp_c_sensor", json!(22.44)).unwrap();
    let stored = read_slot(&store, "temp_c_sensor");
    assert!((stored - 22.44).abs() < 0.0001, "got {stored}");
}

#[test]
fn slot_without_quantity_is_passthrough() {
    let store = fresh_store();
    let path = NodePath::root().child("s");
    store.write_slot(&path, "raw", json!(123.456)).unwrap();
    let stored = read_slot(&store, "raw");
    assert_eq!(stored, 123.456);
}
