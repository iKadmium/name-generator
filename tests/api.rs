use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use name_generator::{app, data::Dataset};
use serde_json::Value;
use tower::ServiceExt;

async fn get(uri: &str) -> (StatusCode, Value) {
    let res = app(Dataset::embedded())
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn single_name() {
    let (status, body) = get("/name?gender=female&year=1987").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["gender"], "female");
    assert_eq!(body["year"], 1987);
    assert_eq!(body["dataset_year"], 1984);
    assert!(body["name"].is_string());
}

#[tokio::test]
async fn single_name_with_forbidden() {
    // 2024 boys: forbid everyone except rank 100 (Leon) would be long, so just
    // check a forbidden name never comes back over many draws.
    for _ in 0..200 {
        let (status, body) = get("/name?gender=boy&year=2024&forbidden=Muhammad,%20noah").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_ne!(body["name"], "Muhammad");
        assert_ne!(body["name"], "Noah");
        assert_eq!(body["gender"], "male");
    }
}

#[tokio::test]
async fn multiple_names() {
    let (status, body) = get("/names?gender=male&year=1950&count=10&forbidden=John,David").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["dataset_year"], 1954);
    let names: Vec<&str> = body["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 10);
    let unique: std::collections::HashSet<_> = names.iter().collect();
    assert_eq!(unique.len(), 10);
    assert!(!names.contains(&"John") && !names.contains(&"David"));
}

#[tokio::test]
async fn too_many_names() {
    let (status, body) = get("/names?gender=female&year=2024&count=100&forbidden=Olivia").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["error"].as_str().unwrap().contains("99"));
}

#[tokio::test]
async fn bad_requests() {
    for uri in [
        "/name?year=1990",
        "/name?gender=other&year=1990",
        "/name?gender=male&year=abc",
        "/names?gender=male&year=1990",
        "/names?gender=male&year=1990&count=0",
        "/names?gender=male&year=1990&count=101",
    ] {
        let (status, body) = get(uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        assert!(body["error"].is_string(), "{uri}: {body}");
    }
}
