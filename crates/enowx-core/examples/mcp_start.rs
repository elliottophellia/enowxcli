//! How long each discovered MCP server takes to start and list its tools.
//! cargo run -q -p enowx-core --example mcp_start -- [WORKSPACE]
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    let workspace = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let discovery = enowx_core::Discovery::run(&workspace);
    for server in discovery.mcp_servers.iter().filter(|s| s.enabled) {
        let started = Instant::now();
        let outcome = tokio::time::timeout(Duration::from_secs(40), async {
            let client = enowx_core::McpClient::spawn(server).await?;
            let handshake = started.elapsed();
            let tools = client.list_tools().await?;
            anyhow::Ok((handshake, tools.len()))
        })
        .await;
        match outcome {
            Ok(Ok((handshake, tools))) => println!(
                "{:<12} handshake {:>6.2}s, {tools} tools listed at {:>6.2}s",
                server.name,
                handshake.as_secs_f64(),
                started.elapsed().as_secs_f64()
            ),
            Ok(Err(error)) => println!(
                "{:<12} FAILED after {:>6.2}s: {error:#}",
                server.name,
                started.elapsed().as_secs_f64()
            ),
            Err(_) => println!("{:<12} still not ready after 40s", server.name),
        }
    }
}
