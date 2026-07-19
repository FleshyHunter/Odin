// auth_guard: an Axum extractor that verifies the access token and
// resolves user_id — handlers just add `AuthUser(user_id): AuthUser`
// as a parameter and get a 401 automatically on anything invalid.
//
// RLS transaction helper: per SCHEMA.md's Row Level Security section,
// this must use set_config() with is_local=true (NOT plain SET, which
// isn't safely parameterizable, and NOT a persistent SET, which would
// leak across the pooled connection into the next request that reuses
// it) inside a real transaction. Exposed as a plain function rather
// than a second extractor — a borrowed sqlx::Transaction doesn't
// express cleanly as a FromRequestParts return type, and there are no
// protected resource routes yet (later blocks) to wire an extractor
// into regardless.
//
// ownership_check: Rule 34's IDOR protection — every resource route
// must verify the resource's owning user_id matches the requester's
// before returning data, never trusting a URL's :id alone.

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    RequestPartsExt,
};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::auth::jwt::{self, TokenType};
use crate::state::AppState;

pub struct AuthUser(pub Uuid);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Missing or invalid Authorization header"))?;

        let claims = jwt::verify(bearer.token(), TokenType::Access, &state.jwt_secret)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid or expired access token"))?;

        let user_id = Uuid::parse_str(&claims.sub)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid token subject"))?;

        Ok(AuthUser(user_id))
    }
}

/// BEGIN + set_config(app.current_user_id, ..., is_local=true) as one
/// step, so every query run against the returned transaction is
/// automatically scoped by RLS to this user — the caller runs its
/// queries, then commits (or rolls back on error).
pub async fn begin_rls_transaction(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('app.current_user_id', $1, true)")
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

/// 404 rather than 403 on mismatch — deliberately doesn't confirm to a
/// requester that a resource they don't own actually exists (an IDOR
/// probe learns nothing either way).
/// Not called anywhere yet — no protected resource routes exist until
/// later blocks (study_threads, journeys, etc.), which is what this is
/// built for (Rule 34).
#[allow(dead_code)]
pub fn ensure_owns(resource_owner_id: Uuid, requester_id: Uuid) -> Result<(), StatusCode> {
    if resource_owner_id == requester_id {
        Ok(())
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
