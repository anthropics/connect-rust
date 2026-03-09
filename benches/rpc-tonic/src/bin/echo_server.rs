//! Minimal echo server for framework-overhead benchmarking (tonic).

use tonic::transport::Server;
use tonic::{Request, Response, Status};

pub mod bench_proto {
    tonic::include_proto!("bench.v1");
}

use bench_proto::echo_service_server::{EchoService, EchoServiceServer};

struct EchoImpl;

#[tonic::async_trait]
impl EchoService for EchoImpl {
    async fn echo(
        &self,
        req: Request<bench_proto::EchoRequest>,
    ) -> Result<Response<bench_proto::EchoResponse>, Status> {
        // Move the string out of the request into the response — no copy.
        let message = req.into_inner().message;
        Ok(Response::new(bench_proto::EchoResponse { message }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let svc = EchoServiceServer::new(EchoImpl);

    let addr = "127.0.0.1:0".parse().unwrap();
    let incoming = tonic::transport::server::TcpIncoming::bind(addr)?.with_nodelay(Some(true));
    let addr = incoming.local_addr()?;
    println!("{addr}");

    Server::builder()
        .add_service(svc)
        .serve_with_incoming_shutdown(incoming, async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;

    Ok(())
}
