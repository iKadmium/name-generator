use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use name_generator::{API_KEY_HEADER, app, data::Dataset};
use serde_json::{Value, json};
use tower::ServiceExt;

const KEY: &str = "test-key";
const PROTOCOL: &str = "2025-06-18";

async fn mcp_http(msg: Value, api_key: Option<&str>) -> (StatusCode, Value) {
    // A Host that isn't loopback, to check host validation is off for deployments.
    let mut req = Request::post("/mcp")
        .header(header::HOST, "names.example.com")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream")
        .header("mcp-protocol-version", PROTOCOL);
    if let Some(key) = api_key {
        req = req.header(API_KEY_HEADER, key);
    }
    let req = req.body(Body::from(msg.to_string())).unwrap();
    let res = app(Dataset::embedded(), KEY.into())
        .oneshot(req)
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, body)
}

fn call(id: u64, arguments: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {"name": "generate_names", "arguments": arguments},
    })
}

#[tokio::test]
async fn http_requires_api_key() {
    let list = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
    for key in [None, Some("wrong")] {
        let (status, _) = mcp_http(list.clone(), key).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{key:?}");
    }
}

#[tokio::test]
async fn http_initialize_and_list_tools() {
    let (status, body) = mcp_http(
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL,
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1"},
            },
        }),
        Some(KEY),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["serverInfo"]["name"], "name-generator");
    assert!(
        body["result"]["capabilities"]["tools"].is_object(),
        "{body}"
    );

    let (status, body) = mcp_http(
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        Some(KEY),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tools = body["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "generate_names");
    let props = &tools[0]["inputSchema"]["properties"];
    assert!(
        props["people"].is_object() && props["forbidden"].is_object(),
        "{body}"
    );
    assert!(tools[0]["outputSchema"].is_object(), "{body}");
}

#[tokio::test]
async fn http_call_tool() {
    let (status, body) = mcp_http(
        call(
            3,
            json!({
                "people": vec![json!({"gender": "male", "year": 1954}); 10],
                "forbidden": ["David"],
            }),
        ),
        Some(KEY),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = &body["result"];
    assert_eq!(result["isError"], false, "{body}");
    let names: Vec<&str> = result["structuredContent"]["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            assert_eq!(n["dataset_year"], 1954);
            n["name"].as_str().unwrap()
        })
        .collect();
    assert_eq!(names.len(), 10);
    assert_eq!(
        names.iter().collect::<std::collections::HashSet<_>>().len(),
        10
    );
    assert!(!names.contains(&"David"));
}

#[tokio::test]
async fn http_call_tool_errors() {
    for args in [
        json!({"people": []}),
        json!({"people": [{"gender": "other", "year": 1990}]}),
        json!({"people": vec![json!({"gender": "female", "year": 2024}); 100], "forbidden": ["Olivia"]}),
    ] {
        let (status, body) = mcp_http(call(4, args.clone()), Some(KEY)).await;
        assert_eq!(status, StatusCode::OK, "{args}: {body}");
        assert_eq!(body["result"]["isError"], true, "{args}: {body}");
    }
}

#[test]
fn stdio_round_trip() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_name-generator"))
        .arg("--stdio")
        .env_remove("API_KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut send = |msg: Value| writeln!(stdin, "{msg}").unwrap();
    let mut recv = || {
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        serde_json::from_str::<Value>(&line).unwrap()
    };

    send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL,
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"},
        },
    }));
    assert_eq!(recv()["result"]["serverInfo"]["name"], "name-generator");
    send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    send(call(
        2,
        json!({"people": [{"gender": "girl", "year": 1987}]}),
    ));
    let body = recv();
    assert_eq!(body["id"], 2);
    let picked = &body["result"]["structuredContent"]["names"][0];
    assert_eq!(picked["gender"], "female", "{body}");
    assert_eq!(picked["dataset_year"], 1984, "{body}");

    drop(stdin);
    assert!(child.wait().unwrap().success());
}
