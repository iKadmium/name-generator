use std::collections::HashSet;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use name_generator::{app, data::Dataset};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn post_raw(body: &str, content_type: &str) -> (StatusCode, Value) {
    let req = Request::post("/names")
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(body.to_owned()))
        .unwrap();
    let res = app(Dataset::embedded()).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn post(body: Value) -> (StatusCode, Value) {
    post_raw(&body.to_string(), "application/json").await
}

fn names(body: &Value) -> Vec<&str> {
    body["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn single_person() {
    let (status, body) = post(json!({"people": [{"gender": "female", "year": 1987}]})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let n = &body["names"][0];
    assert_eq!(n["gender"], "female");
    assert_eq!(n["year"], 1987);
    assert_eq!(n["dataset_year"], 1984);
    assert!(n["name"].is_string());
}

#[tokio::test]
async fn mixed_people_in_order() {
    let (status, body) = post(json!({
        "people": [
            {"gender": "boy", "year": 1950},
            {"gender": "f", "year": 2024},
            {"gender": "male", "year": 2024},
        ]
    }))
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let got: Vec<(&str, u64)> = body["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            (
                n["gender"].as_str().unwrap(),
                n["dataset_year"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(got, [("male", 1954), ("female", 2024), ("male", 2024)]);
}

#[tokio::test]
async fn forbidden_and_distinct() {
    for _ in 0..50 {
        let (status, body) = post(json!({
            "people": vec![json!({"gender": "male", "year": 1954}); 20],
            "forbidden": ["DAVID", "john"],
        }))
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let names = names(&body);
        assert_eq!(names.iter().collect::<HashSet<_>>().len(), 20);
        assert!(!names.contains(&"David") && !names.contains(&"John"));
    }
}

#[tokio::test]
async fn runs_out_of_names() {
    let (status, body) = post(json!({
        "people": vec![json!({"gender": "female", "year": 2024}); 100],
        "forbidden": ["Olivia"],
    }))
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["error"].as_str().unwrap().contains("people[99]"),
        "{body}"
    );
}

#[tokio::test]
async fn invalid_bodies() {
    let too_many = json!({"people": vec![json!({"gender": "male", "year": 1990}); 101]});
    for (body, expected) in [
        (json!({}), StatusCode::UNPROCESSABLE_ENTITY),
        (json!({"people": []}), StatusCode::UNPROCESSABLE_ENTITY),
        (too_many, StatusCode::UNPROCESSABLE_ENTITY),
        (
            json!({"people": [{"year": 1990}]}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({"people": [{"gender": "other", "year": 1990}]}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({"people": [{"gender": "male", "year": "1990"}]}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({"people": [{"gender": "male", "year": 1990}], "forbiden": []}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        let (status, resp) = post(body.clone()).await;
        assert_eq!(status, expected, "{body}: {resp}");
        assert!(resp["error"].is_string(), "{body}: {resp}");
    }

    let (status, resp) = post_raw("{not json", "application/json").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{resp}");
    let (status, resp) = post_raw("{}", "text/plain").await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{resp}");
}
