use std::sync::Arc;

use name_generator::mcp::NameServer;
use name_generator::{app, data::Dataset};
use rmcp::ServiceExt;
use tokio::signal;

const USAGE: &str = "\
Usage: name-generator [--stdio]

  (no args)  Serve the REST API and MCP (at /mcp) over HTTP.
             Requires API_KEY; binds to BIND_ADDR (default 0.0.0.0:3000).
  --stdio    Serve MCP over stdin/stdout instead. No API key needed.";

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [] => serve_http().await,
        ["--stdio"] => serve_stdio().await,
        ["-h" | "--help"] => println!("{USAGE}"),
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}

async fn serve_stdio() {
    let server = NameServer::new(Arc::new(Dataset::embedded()))
        .serve(rmcp::transport::stdio())
        .await
        .unwrap_or_else(|e| {
            eprintln!("failed to start MCP server: {e}");
            std::process::exit(1);
        });
    if let Err(e) = server.waiting().await {
        eprintln!("MCP server error: {e}");
        std::process::exit(1);
    }
}

async fn serve_http() {
    let api_key = match std::env::var("API_KEY") {
        Ok(key) if !key.trim().is_empty() => key,
        _ => {
            eprintln!("API_KEY must be set to a non-empty value");
            std::process::exit(1);
        }
    };
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));
    println!("listening on http://{addr}");
    axum::serve(listener, app(Dataset::embedded(), api_key))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

/// Resolves on Ctrl+C or SIGTERM (what `docker stop` sends).
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
