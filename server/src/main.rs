use anyhow::Result;
use axum::Router;
use clap::Parser;
use std::net::SocketAddr;
use tracing_subscriber::{fmt, EnvFilter};
use server::build_app;
use tokio::net::TcpListener;

#[derive(Parser)]
struct Args {
    /// Index directory path
    #[arg(long, default_value = "./index")]
    index: String,
    /// Host to bind
    #[arg(long, default_value = "0.0.0.0")]
    host: String,
    /// Port to bind
    #[arg(long, default_value_t = 8080)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    fmt().with_env_filter(EnvFilter::from_default_env()).init();
    let args = Args::parse();
    let app: Router = build_app(args.index.clone())?;

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "server listening");
    axum::serve(listener, app).await?;
    Ok(())
}
