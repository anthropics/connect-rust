//! Log-ingest server for decode-heavy profiling (tonic/prost).
//!
//! Matches the connectrpc-rs `log_server` handler field-for-field so the
//! only variable is the proto library and framework. The key difference:
//! prost fully decodes `LogRequest` into owned `Vec<LogRecord>` with a
//! `String` alloc for every string field on every record BEFORE the
//! handler runs. The handler then reads from already-decoded owned data.

use tonic::transport::Server;
use tonic::{Request, Response, Status};

pub mod bench_proto {
    tonic::include_proto!("bench.v1");
}

use bench_proto::log_ingest_service_server::{LogIngestService, LogIngestServiceServer};
use bench_proto::{LogIngestResponse, LogRequest};

struct LogIngestImpl;

#[tonic::async_trait]
impl LogIngestService for LogIngestImpl {
    async fn ingest(
        &self,
        request: Request<LogRequest>,
    ) -> Result<Response<LogIngestResponse>, Status> {
        // Prost decode has already run and allocated every String.
        // The iteration below is just summing over already-materialized data.
        let req = request.into_inner();

        let mut count = 0i32;
        let mut total_message_bytes = 0i64;
        let mut total_label_bytes = 0i64;
        let mut max_severity = 0i32;

        for rec in &req.records {
            count += 1;

            if rec.severity > max_severity {
                max_severity = rec.severity;
            }

            total_message_bytes += rec.message.len() as i64;
            total_message_bytes += rec.service_name.len() as i64;
            total_message_bytes += rec.instance_id.len() as i64;
            total_message_bytes += rec.trace_id.len() as i64;
            total_message_bytes += rec.span_id.len() as i64;

            if let Some(ref src) = rec.source {
                total_message_bytes += src.file.len() as i64;
                total_message_bytes += src.function.len() as i64;
                let _ = src.line;
            }

            for (k, v) in &rec.labels {
                total_label_bytes += (k.len() + v.len()) as i64;
            }
        }

        Ok(Response::new(LogIngestResponse {
            count,
            total_message_bytes,
            total_label_bytes,
            max_severity,
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let svc = LogIngestServiceServer::new(LogIngestImpl);

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
