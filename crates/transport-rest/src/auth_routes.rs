//! `/api/v1/auth/*` — identity introspection.
//!
//! `GET /api/v1/auth/whoami` returns the resolved [`spi::AuthContext`]
//! for the caller: which actor, which tenant, which scopes. Useful for:
//!
//!   * Smoke-testing the auth seam from a shell (`agent auth whoami`).
//!   * Studio deciding whether to show a "sign in" button (if the
//!     provider is `dev_null`, nobody's really signed in).
//!   * CI + contract tests verifying a token maps to the expected
//!     context without having to mutate state.
//!
//! This is the first route that *requires* the `AuthContext` extractor
//! to resolve before the handler runs — the whole point of the
//! endpoint is to surface the resolved identity.

use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use spi::{Actor, AuthContext, Scope};

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/auth/whoami", get(whoami))
}

/// Wire shape returned by `GET /api/v1/auth/whoami`.
///
/// Serialises with stable field order — do not reorder per
/// [NEW-API.md § "Rules for the wire shape"](../../docs/design/NEW-API.md).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhoAmIDto {
    /// `"user" | "machine" | "dev_null"` — matches
    /// `spi::Actor`'s serde tag.
    pub actor_kind: &'static str,
    /// Stable NodeId for `user`/`machine`; `null` for `dev_null`.
    pub actor_id: Option<String>,
    /// Display string: user's display name, machine label, or
    /// `"local-dev-null"`.
    pub actor_display: String,
    /// Tenant the request is scoped to.
    pub tenant: String,
    /// Scopes the actor holds. Stable order (enum declaration order).
    pub scopes: Vec<Scope>,
    /// Identifier for the resolving provider (e.g. `"dev_null"`,
    /// `"static_token"`). Studio branches on this to decide whether
    /// auth is truly configured.
    pub provider: String,
}

async fn whoami(
    ctx: AuthContext,
    axum::extract::State(s): axum::extract::State<AppState>,
) -> Json<WhoAmIDto> {
    let (actor_kind, actor_id, actor_display) = match &ctx.actor {
        Actor::User { id, display_name } => ("user", Some(id.to_string()), display_name.clone()),
        Actor::Machine { id, label } => ("machine", Some(id.to_string()), label.clone()),
        Actor::DevNull => ("dev_null", None, "local-dev-null".to_string()),
    };

    // Enumerate every known scope and include those the context grants;
    // this gives the client a stable order (enum declaration order)
    // without exposing the internal bitflag representation.
    let all = [
        Scope::ReadNodes,
        Scope::WriteNodes,
        Scope::WriteSlots,
        Scope::WriteConfig,
        Scope::ManagePlugins,
        Scope::ManageFleet,
        Scope::Admin,
    ];
    let scopes: Vec<Scope> = all
        .into_iter()
        .filter(|s| ctx.scopes.contains(*s))
        .collect();

    Json(WhoAmIDto {
        actor_kind,
        actor_id,
        actor_display,
        tenant: ctx.tenant.as_str().to_string(),
        scopes,
        provider: s.auth_provider.id().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use engine::BehaviorRegistry;
    use blocks_host::BlockRegistry;
    use graph::{seed as graph_seed, GraphStore, KindRegistry, NullSink};
    use spi::{KindId, NodePath};
    use tokio::sync::broadcast;

    fn state() -> AppState {
        let kinds = KindRegistry::new();
        graph_seed::register_builtins(&kinds);
        let graph = Arc::new(GraphStore::new(kinds, Arc::new(NullSink)));
        graph.create_root(KindId::new("sys.core.station")).unwrap();
        let (events, _) = broadcast::channel(16);
        let (behaviors, _) = BehaviorRegistry::new(graph.clone());
        let _ = NodePath::root(); // keep import used
        AppState::new(graph, behaviors, events, BlockRegistry::new())
    }

    /// Dev-null-provider default state → actor kind "dev_null", all
    /// scopes (admin implies), provider id `"dev_null"`.
    #[tokio::test]
    async fn dev_null_whoami_returns_admin_stamp() {
        let s = state();
        let ctx = AuthContext::dev_null();
        let Json(dto) = whoami(ctx, axum::extract::State(s)).await;
        assert_eq!(dto.actor_kind, "dev_null");
        assert_eq!(dto.actor_id, None);
        assert_eq!(dto.actor_display, "local-dev-null");
        assert_eq!(dto.tenant, "default");
        assert!(dto.scopes.contains(&Scope::Admin));
        assert_eq!(dto.provider, "dev_null");
    }
}
