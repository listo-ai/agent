//! `GET /api/v1/units` — the public quantity / unit registry.
//!
//! Clients use this to drive unit-picker UIs and to cache conversion
//! metadata keyed off the returned version / ETag (versioning lands
//! alongside the platform release per
//! `agent/docs/design/USER-PREFERENCES.md` § "Enum versioning and
//! canonical-unit migration").
//!
//! The route is intentionally unauthenticated — no tenant data, only
//! the static registry shape. Matches the design doc's "public
//! quantity/unit registry, so clients can render labels and offer
//! unit-picker UIs without hard-coding."
//!
//! Rule I: the handler is four lines — it asks [`spi::registry_dto`]
//! for the shape and serialises. All registry logic lives in spi; a
//! swap to gRPC or fleet-transport would need to change only the
//! mount.

use axum::routing::get;
use axum::{Json, Router};

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/units", get(get_units))
}

async fn get_units() -> Json<spi::RegistryDto> {
    Json(spi::registry_dto(spi::default_registry()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_registry_dto_with_every_quantity() {
        let Json(dto) = get_units().await;
        // 11 quantities at the time of writing; we assert the minimum
        // rather than the exact count so adding a new quantity doesn't
        // ripple into this test.
        assert!(
            dto.quantities.len() >= 11,
            "expected at least 11 quantities, got {}",
            dto.quantities.len()
        );
        // Temperature is always first and always includes Celsius.
        let temp = &dto.quantities[0];
        assert_eq!(temp.id, spi::Quantity::Temperature);
        assert!(temp.allowed.contains(&spi::Unit::Celsius));
    }
}
