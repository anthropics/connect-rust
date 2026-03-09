//! Fortune benchmark logic adapted from the TechEmpower Web Framework Benchmarks.
//!
//! Backed by a valkey instance running as a sibling container. Unlike the
//! original in-memory SQLite approach (single-Mutex bottleneck that hid
//! framework differences at ~100k rps), valkey gives a real network
//! round-trip over a connection that pipelines concurrent requests without
//! contention. This keeps the "backend round-trip + sort + encode" work shape
//! of TechEmpower's fortunes test without the heavyweight postgres setup.

use std::sync::atomic::{AtomicUsize, Ordering};

use redis::AsyncTypedCommands;
use redis::aio::MultiplexedConnection;

/// The 12 standard TFB fortune messages.
pub const FORTUNES: &[(i32, &str)] = &[
    (1, "fortune: No such file or directory"),
    (
        2,
        "A computer scientist is someone who fixes things that aren\u{2019}t broken.",
    ),
    (3, "After enough decimal places, nobody gives a damn."),
    (
        4,
        "A bad random number generator: 1, 1, 1, 1, 1, 4.33e+67, 1, 1, 1",
    ),
    (
        5,
        "A computer program does what you tell it to do, not what you want it to do.",
    ),
    (
        6,
        "Emacs is a nice operating system, but I prefer UNIX. \u{2014} Tom Christaensen",
    ),
    (7, "Any program that runs right is obsolete."),
    (
        8,
        "A list is only as strong as its weakest link. \u{2014} Donald Knuth",
    ),
    (9, "Feature: A bug with seniority."),
    (10, "Computers make very fast, very accurate mistakes."),
    (
        11,
        "<script>alert(\"This should not be displayed in a browser alert box.\");</script>",
    ),
    (
        12,
        "\u{30D5}\u{30EC}\u{30FC}\u{30E0}\u{30EF}\u{30FC}\u{30AF}\u{306E}\u{30D9}\u{30F3}\u{30C1}\u{30DE}\u{30FC}\u{30AF}",
    ),
];

const KEY: &str = "fortunes";

/// Open a pipelined async connection to valkey at `addr`.
///
/// `MultiplexedConnection` is `Clone + Send + Sync`: each clone shares the
/// same underlying TCP connection, commands pipeline through an internal
/// mpsc channel with response demuxing. The crate default channel capacity
/// is 50 — at c>50 workers block on send, so we bump to 512 to cover all
/// benchmark concurrency levels.
pub async fn connect(addr: &str) -> redis::RedisResult<MultiplexedConnection> {
    let client = redis::Client::open(format!("redis://{addr}"))?;
    let cfg = redis::AsyncConnectionConfig::new().set_pipeline_buffer_size(512);
    client
        .get_multiplexed_async_connection_with_config(&cfg)
        .await
}

/// N `MultiplexedConnection`s with atomic round-robin selection.
///
/// At high fan-in (~70k rps on this workload), a single multiplexed
/// connection bottlenecks on its background demux task; N connections
/// give N independent pipelines. Matches the go-redis pool topology.
pub struct ValkeyPool {
    conns: Vec<MultiplexedConnection>,
    next: AtomicUsize,
}

impl ValkeyPool {
    pub async fn connect(addr: &str, n: usize) -> redis::RedisResult<Self> {
        assert!(n > 0, "ValkeyPool requires at least one connection");
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

    /// Round-robin a connection handle. Clones the selected
    /// `MultiplexedConnection` (cheap: just an `mpsc::Sender` clone).
    pub fn get(&self) -> MultiplexedConnection {
        let idx = self.next.fetch_add(1, Ordering::Relaxed) % self.conns.len();
        self.conns[idx].clone()
    }
}

/// Populate the `fortunes` hash with the 12 standard messages.
/// Idempotent — safe to call on an already-seeded instance.
pub async fn seed(conn: &mut MultiplexedConnection) -> redis::RedisResult<()> {
    let mut pipe = redis::pipe();
    for &(id, message) in FORTUNES {
        pipe.hset(KEY, id, message);
    }
    pipe.query_async(conn).await
}

/// Fetch all fortunes, add the ephemeral fortune, sort by message.
///
/// HGETALL returns `(field, value)` pairs; field is the stringified id.
pub async fn query_fortunes(
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Requires a running valkey at `VALKEY_TEST_ADDR` (or 127.0.0.1:6379).
    /// Skips (passes vacuously) if not reachable.
    #[tokio::test]
    async fn roundtrip_when_available() {
        let addr =
            std::env::var("VALKEY_TEST_ADDR").unwrap_or_else(|_| "127.0.0.1:6379".to_string());
        let Ok(mut conn) = connect(&addr).await else {
            eprintln!("no valkey at {addr}, skipping");
            return;
        };
        seed(&mut conn).await.unwrap();
        let fortunes = query_fortunes(&mut conn).await.unwrap();
        assert_eq!(fortunes.len(), 13);
        assert!(
            fortunes
                .iter()
                .any(|(_, m)| m == "Additional fortune added at request time.")
        );
        for w in fortunes.windows(2) {
            assert!(w[0].1 <= w[1].1, "not sorted: {:?} > {:?}", w[0].1, w[1].1);
        }
    }
}
