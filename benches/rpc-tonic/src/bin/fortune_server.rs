use std::sync::atomic::{AtomicUsize, Ordering};

use redis::AsyncTypedCommands;
use redis::aio::MultiplexedConnection;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

pub mod fortune_proto {
    tonic::include_proto!("fortune.v1");
}

use fortune_proto::fortune_service_server::{FortuneService, FortuneServiceServer};

// ── Inline fortune logic ────────────────────────────────────────────
// Duplicated from `rpc-bench::fortune` (benches/rpc/src/fortune.rs) to
// avoid a cross-crate dep. KEEP IN SYNC: if ValkeyPool or query_fortunes
// change there, mirror here.

const KEY: &str = "fortunes";
const VALKEY_POOL_SIZE: usize = 8;

struct ValkeyPool {
    conns: Vec<MultiplexedConnection>,
    next: AtomicUsize,
}

impl ValkeyPool {
    async fn connect(addr: &str, n: usize) -> redis::RedisResult<Self> {
        let client = redis::Client::open(format!("redis://{addr}"))?;
        let cfg = redis::AsyncConnectionConfig::new().set_pipeline_buffer_size(512);
        let mut conns = Vec::with_capacity(n);
        for _ in 0..n {
            conns.push(
                client
                    .get_multiplexed_async_connection_with_config(&cfg)
                    .await?,
            );
        }
        Ok(Self {
            conns,
            next: AtomicUsize::new(0),
        })
    }

    fn get(&self) -> MultiplexedConnection {
        let idx = self.next.fetch_add(1, Ordering::Relaxed) % self.conns.len();
        self.conns[idx].clone()
    }
}

async fn query_fortunes(
    conn: &mut MultiplexedConnection,
) -> redis::RedisResult<Vec<(i32, String)>> {
    let raw = conn.hgetall(KEY).await?;
    let mut fortunes: Vec<(i32, String)> = raw
        .into_iter()
        .map(|(id, msg): (String, String)| (id.parse().unwrap_or(0), msg))
        .collect();
    fortunes.push((0, "Additional fortune added at request time.".to_string()));
    fortunes.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(fortunes)
}

// ── Tonic service implementation ──

struct FortuneServiceImpl {
    pool: ValkeyPool,
}

#[tonic::async_trait]
impl FortuneService for FortuneServiceImpl {
    async fn get_fortunes(
        &self,
        _req: Request<fortune_proto::GetFortunesRequest>,
    ) -> Result<Response<fortune_proto::GetFortunesResponse>, Status> {
        let mut conn = self.pool.get();
        let fortunes = query_fortunes(&mut conn)
            .await
            .map_err(|e| Status::internal(format!("valkey: {e}")))?;

        Ok(Response::new(fortune_proto::GetFortunesResponse {
            fortunes: fortunes
                .into_iter()
                .map(|(id, message)| fortune_proto::Fortune { id, message })
                .collect(),
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let valkey_addr = std::env::args()
        .nth(1)
        .expect("usage: fortune-server-tonic <valkey_addr>");
    let pool = ValkeyPool::connect(&valkey_addr, VALKEY_POOL_SIZE).await?;

    let svc = FortuneServiceServer::new(FortuneServiceImpl { pool });

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
