use name_generator::{app, data::Dataset};

#[tokio::main]
async fn main() {
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));
    println!("listening on http://{addr}");
    axum::serve(listener, app(Dataset::embedded()))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("server error");
}
