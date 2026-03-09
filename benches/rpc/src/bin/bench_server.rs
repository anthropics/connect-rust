use std::sync::Arc;

use connectrpc::Router;
use rpc_bench::{BenchServiceExt, BenchServiceImpl};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let router = Router::new();
    let router = Arc::new(BenchServiceImpl).register(router);

    let bound = connectrpc::server::Server::bind("127.0.0.1:0").await?;
    let addr = bound.local_addr()?;
    // Print the address to stdout for the benchmark harness.
    println!("{addr}");

    bound.serve(router).await?;
    Ok(())
}
