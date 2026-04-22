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
        assert_eq!(temp.label, "Temperature");
        // Flat units table is populated with labels + symbols so
        // clients (Studio, CLI, block-ui-sdk) can render without
        // hard-coding.
        assert!(!dto.units.is_empty(), "units table empty");
        let celsius = dto
            .units
            .iter()
            .find(|u| u.id == spi::Unit::Celsius)
            .expect("Celsius present in flat units table");
        assert_eq!(celsius.symbol, "°C");
        assert_eq!(celsius.label, "Degrees Celsius");
        // Celsius is temperature's canonical → identity coefficients.
        let coeffs = celsius
            .to_canonical
            .expect("allowed units carry affine coefficients");
        assert!((coeffs.scale - 1.0).abs() < 1e-12);
        assert!(coeffs.offset.abs() < 1e-12);

        // Fahrenheit → known non-identity coefficients (5/9 and
        // −160/9). Round-trip test against the server's own
        // `spi::default_registry()` is already covered in
        // `listo-spi::units::tests`; here we just assert the wire
        // shape has them, so a drop in serialisation would fail
        // fast at this level too.
        let f = dto
            .units
            .iter()
            .find(|u| u.id == spi::Unit::Fahrenheit)
            .unwrap();
        assert!(f.to_canonical.is_some());
    }
}
