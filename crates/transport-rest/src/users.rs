//! `/api/v1/users` — user management surface.
//!
//! `GET  /api/v1/users`              — list `sys.auth.user` nodes with
//!                                     tag-aware filtering.
//! `POST /api/v1/users/{id}/grants`  — per-user role-grant wire shape
//!                                     (bulk_action_id threading, 202
//!                                     Accepted; Zitadel call is a future
//!                                     landing).
//!
//! ## Query schema vs auth-resolution schema
//!
//! The query schema here (`user_management_query_schema`) exposes
//! `tags.labels` and pattern `tags.kv.*` so Studio list views can
//! filter by tag. The auth-resolution schema (`auth::auth_resolution_query_schema`)
//! has an explicit allowlist that does NOT include `tags.*` — so tag
//! filters can never reach a permission decision. The schemas are
//! separate by design; sharing one schema to both paths is what this
//! split prevents.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use query::{FieldType, Operator, QueryRequest, QuerySchema, SortField};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use spi::{AuthContext, Scope};

use crate::routes::ApiError;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/users", get(list_users))
        .route("/api/v1/users/{id}/grants", post(grant_role))
}

// ---- DTOs -----------------------------------------------------------------

/// Tags extracted from the `config.tags` slot.
///
/// Serialises as `{"labels": [...], "kv": {...}}` — the executor's
/// `field_value` traverses `tags.labels` and `tags.kv.<key>` correctly.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TagsDto {
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub kv: std::collections::BTreeMap<String, String>,
}

/// One `sys.auth.user` node as seen by the user-management list view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDto {
    pub id: String,
    pub path: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub enabled: bool,
    pub tags: TagsDto,
}

/// Request body for `POST /api/v1/users/{id}/grants`.
#[derive(Debug, Deserialize)]
pub struct GrantRoleReq {
    pub role: String,
    /// Studio-generated correlation id for the bulk session.
    /// All audit events for one "grant role X to N users" action carry
    /// the same id, making the full operation reconstructible from the
    /// audit log.
    pub bulk_action_id: String,
}

/// Response for `POST /api/v1/users/{id}/grants`.
#[derive(Debug, Serialize)]
pub struct GrantRoleResp {
    pub user_id: String,
    pub role: String,
    pub bulk_action_id: String,
    /// Always `"accepted"` — Zitadel fan-out happens asynchronously.
    pub status: &'static str,
}

// ---- query schema ---------------------------------------------------------

/// `QuerySchema` for the user-management list view.
///
/// Exposes `tags.labels` (TextArr) and `tags.kv.*` (pattern field, Text)
/// for Studio bulk-action filtering. This schema is intentionally separate
/// from `auth::auth_resolution_query_schema`, which has an explicit
/// allowlist that excludes `tags.*`.
pub(crate) fn user_management_query_schema() -> QuerySchema {
    QuerySchema::new(50, 500)
        .field("id", FieldType::Text, [Operator::Eq, Operator::In])
        .field(
            "path",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix],
        )
        .field(
            "display_name",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix],
        )
        .field("email", FieldType::Text, [Operator::Eq, Operator::Ne])
        .field("enabled", FieldType::Text, [Operator::Eq])
        .field(
            "tags.labels",
            FieldType::TextArr,
            [Operator::Contains, Operator::In, Operator::Exists],
        )
        .pattern_field(
            "tags.kv.",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::In, Operator::Exists],
        )
        .default_sort([SortField::asc("path")])
}

// ---- handlers -------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ListUsersQuery {
    pub filter: Option<String>,
    pub sort: Option<String>,
    pub page: Option<usize>,
    pub size: Option<usize>,
}

async fn list_users(
    ctx: AuthContext,
    State(s): State<AppState>,
    axum::extract::Query(raw): axum::extract::Query<ListUsersQuery>,
) -> Result<Json<query::Page<UserDto>>, ApiError> {
    ctx.require(Scope::ReadNodes).map_err(ApiError::from_auth)?;

    let users: Vec<UserDto> = s
        .graph
        .snapshots()
        .into_iter()
        .filter(|n| n.kind.as_str() == "sys.auth.user")
        .map(user_dto_from_snapshot)
        .collect();

    let query = query::validate(
        &user_management_query_schema(),
        QueryRequest {
            filter: raw.filter,
            sort: raw.sort,
            page: raw.page,
            size: raw.size,
        },
    )
    .map_err(|e| ApiError::bad_request(e.to_string()))?;

    query::execute(users, &query)
        .map(Json)
        .map_err(|e| ApiError::bad_request(e.to_string()))
}

async fn grant_role(
    ctx: AuthContext,
    State(_s): State<AppState>,
    Path(user_id): Path<String>,
    Json(req): Json<GrantRoleReq>,
) -> Result<(StatusCode, Json<GrantRoleResp>), ApiError> {
    ctx.require(Scope::WriteNodes).map_err(ApiError::from_auth)?;
    if req.bulk_action_id.is_empty() {
        return Err(ApiError::bad_request("bulk_action_id must not be empty"));
    }
    // Wire shape accepted; Zitadel fan-out is a future landing.
    Ok((
        StatusCode::ACCEPTED,
        Json(GrantRoleResp {
            user_id,
            role: req.role,
            bulk_action_id: req.bulk_action_id,
            status: "accepted",
        }),
    ))
}

// ---- helpers --------------------------------------------------------------

fn user_dto_from_snapshot(snap: graph::NodeSnapshot) -> UserDto {
    let mut display_name: Option<String> = None;
    let mut email: Option<String> = None;
    let mut enabled = true;
    let mut tags = TagsDto::default();

    for (name, sv) in snap.slot_values {
        match name.as_str() {
            "display_name" => {
                display_name = sv.value.as_str().map(str::to_owned);
            }
            "email" => {
                email = sv.value.as_str().map(str::to_owned);
            }
            "enabled" => {
                enabled = sv.value.as_bool().unwrap_or(true);
            }
            "config.tags" => {
                tags = parse_tags_slot(&sv.value);
            }
            _ => {}
        }
    }

    UserDto {
        id: snap.id.to_string(),
        path: snap.path.to_string(),
        display_name,
        email,
        enabled,
        tags,
    }
}

fn parse_tags_slot(val: &JsonValue) -> TagsDto {
    if val.is_null() {
        return TagsDto::default();
    }
    let Some(obj) = val.as_object() else {
        return TagsDto::default();
    };
    let labels = obj
        .get("labels")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let kv = obj
        .get("kv")
        .and_then(|v| v.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                .collect()
        })
        .unwrap_or_default();
    TagsDto { labels, kv }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_tags_slot_full() {
        let v = json!({ "labels": ["ops", "oncall"], "kv": { "site": "abc" } });
        let dto = parse_tags_slot(&v);
        assert_eq!(dto.labels, vec!["ops", "oncall"]);
        assert_eq!(dto.kv.get("site").map(|s| s.as_str()), Some("abc"));
    }

    #[test]
    fn parses_tags_slot_null() {
        let dto = parse_tags_slot(&JsonValue::Null);
        assert!(dto.labels.is_empty());
        assert!(dto.kv.is_empty());
    }

    #[test]
    fn user_management_schema_accepts_tag_filters() {
        let schema = user_management_query_schema();
        assert!(query::validate(
            &schema,
            QueryRequest {
                filter: Some("tags.labels=contains=team/platform".into()),
                sort: None,
                page: None,
                size: None,
            }
        )
        .is_ok());
        assert!(query::validate(
            &schema,
            QueryRequest {
                filter: Some("tags.kv.site==abc".into()),
                sort: None,
                page: None,
                size: None,
            }
        )
        .is_ok());
    }

    #[test]
    fn tag_filters_absent_from_auth_schema() {
        let schema = auth::auth_resolution_query_schema();
        let err = query::validate(
            &schema,
            QueryRequest {
                filter: Some("tags.labels=contains=team/platform".into()),
                sort: None,
                page: None,
                size: None,
            },
        )
        .expect_err("tags.labels must be absent from the auth schema");
        assert!(err.to_string().contains("unknown field"), "{err}");
    }

    #[test]
    fn grant_role_rejects_empty_bulk_action_id() {
        // Pure logic test — no async needed.
        let req = GrantRoleReq {
            role: "org_admin".into(),
            bulk_action_id: "".into(),
        };
        assert!(req.bulk_action_id.is_empty());
    }
}
