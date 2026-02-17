use axum::{
    routing::post,
    Json, Router,
};
use clap::Parser;
use std::net::SocketAddr;

mod methods;
mod utxo_synth;
mod tx_translator;

#[derive(Parser)]
#[command(name = "sydney-btcrpc")]
#[command(about = "Bitcoin RPC compatibility layer for Sydney chain")]
struct Args {
    /// Listen address for JSON-RPC server
    #[arg(long, default_value = "127.0.0.1")]
    listen_addr: String,

    /// Listen port
    #[arg(long, default_value = "8332")]
    port: u16,

    /// Sydney chain RPC endpoint
    #[arg(long)]
    chain_rpc: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let args = Args::parse();

    let addr = format!("{}:{}", args.listen_addr, args.port);
    let socket_addr: SocketAddr = addr.parse()?;

    println!("Sydney Bitcoin RPC Compatibility Layer");
    println!("=====================================");
    println!("Listen: http://{}", addr);
    println!("Chain RPC: {}", args.chain_rpc);
    println!();

    // Build router with JSON-RPC endpoint
    let app = Router::new()
        .route("/", post(handle_rpc));

    let listener = tokio::net::TcpListener::bind(&socket_addr).await?;
    println!("✓ Listening on http://{}", socket_addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn handle_rpc(
    Json(_payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    // TODO: Implement JSON-RPC request routing
    Json(serde_json::json!({
        "jsonrpc": "2.0",
        "error": {
            "code": -32601,
            "message": "Method not found"
        }
    }))
}
