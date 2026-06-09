//! Minimal reflection-only server for poking with external tooling:
//!
//! ```sh
//! buf build -o /tmp/reflection.fds.bin
//! cargo run -p connectrpc-reflection --example reflection_server -- \
//!     /tmp/reflection.fds.bin 127.0.0.1:50051
//! buf curl --protocol grpc --http2-prior-knowledge \
//!     -d '{"listServices": ""}' \
//!     http://127.0.0.1:50051/grpc.reflection.v1.ServerReflection/ServerReflectionInfo
//! ```

use connectrpc::Router;
use connectrpc_reflection::{Reflector, install};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let fds_path = args
        .next()
        .ok_or("usage: reflection_server <fds-file> [addr]")?;
    let addr = args.next().unwrap_or_else(|| "127.0.0.1:50051".to_owned());

    let bytes = std::fs::read(&fds_path)?;
    let reflector = Reflector::from_descriptor_set_bytes(&bytes)?;
    println!("serving {reflector:?} on {addr}");

    let router = install(Router::new(), reflector);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, router.into_axum_router()).await?;
    Ok(())
}
