pub mod data;

use std::sync::Arc;

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use data::{Dataset, Gender, PickError};

/// Upper bound on `count` for `/names`.
pub const MAX_COUNT: usize = 100;

type AppState = Arc<Dataset>;

pub fn app(dataset: Dataset) -> Router {
    Router::new()
        .route("/name", get(name))
        .route("/names", get(names))
        .with_state(Arc::new(dataset))
}

/// Query parameters shared by both endpoints. (Not split into two structs with
/// `#[serde(flatten)]`, which breaks number parsing from query strings.)
#[derive(Deserialize)]
struct NameQuery {
    gender: Gender,
    year: i32,
    /// Comma-separated names to exclude.
    forbidden: Option<String>,
    /// Number of names; required by `/names`, ignored by `/name`.
    count: Option<usize>,
}

#[derive(Serialize)]
struct NameResponse {
    name: String,
    gender: Gender,
    year: i32,
    dataset_year: u16,
}

#[derive(Serialize)]
struct NamesResponse {
    names: Vec<String>,
    gender: Gender,
    year: i32,
    dataset_year: u16,
}

pub struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        #[derive(Serialize)]
        struct Body {
            error: String,
        }
        (self.0, Json(Body { error: self.1 })).into_response()
    }
}

impl From<QueryRejection> for ApiError {
    fn from(r: QueryRejection) -> Self {
        Self(StatusCode::BAD_REQUEST, r.body_text())
    }
}

impl From<PickError> for ApiError {
    fn from(e: PickError) -> Self {
        Self(StatusCode::UNPROCESSABLE_ENTITY, e.to_string())
    }
}

fn parse_forbidden(s: Option<&str>) -> Vec<String> {
    s.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
        .collect()
}

async fn name(
    State(ds): State<AppState>,
    q: Result<Query<NameQuery>, QueryRejection>,
) -> Result<Json<NameResponse>, ApiError> {
    let Query(q) = q?;
    let forbidden = parse_forbidden(q.forbidden.as_deref());
    let mut picked = ds.pick(q.gender, q.year, &forbidden, 1, &mut rand::rng())?;
    Ok(Json(NameResponse {
        name: picked.names.pop().expect("picked exactly one"),
        gender: q.gender,
        year: q.year,
        dataset_year: picked.dataset_year,
    }))
}

async fn names(
    State(ds): State<AppState>,
    q: Result<Query<NameQuery>, QueryRejection>,
) -> Result<Json<NamesResponse>, ApiError> {
    let Query(q) = q?;
    let count = q
        .count
        .ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "missing field `count`".into()))?;
    if !(1..=MAX_COUNT).contains(&count) {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            format!("count must be between 1 and {MAX_COUNT}"),
        ));
    }
    let forbidden = parse_forbidden(q.forbidden.as_deref());
    let picked = ds.pick(q.gender, q.year, &forbidden, count, &mut rand::rng())?;
    Ok(Json(NamesResponse {
        names: picked.names,
        gender: q.gender,
        year: q.year,
        dataset_year: picked.dataset_year,
    }))
}
