//! Streaming-tour server: NumberService implementation covering all four
//! ConnectRPC RPC types (unary, server stream, client stream, bidi stream).
//!
//! Run with:
//!
//! ```sh
//! cargo run -p streaming-tour-example --bin streaming-tour-server
//! ```
//!
//! Then in another terminal:
//!
//! ```sh
//! cargo run -p streaming-tour-example --bin streaming-tour-client
//! ```

use std::pin::Pin;
use std::sync::Arc;

use buffa::view::OwnedView;
use connectrpc::{ConnectError, Context, Router};
use futures::{Stream, StreamExt};

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/_connectrpc.rs"));
}

use proto::anthropic::connectrpc::tour::v1::__buffa::view::*;
use proto::anthropic::connectrpc::tour::v1::*;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

// Local type aliases that flatten the streaming-handler signatures.
// The verbose `Pin<Box<dyn Stream<...> + Send>>` form is what the
// generated traits expect today; these aliases are pure sugar at the
// call site (pending broader handler-trait ergonomics work).
type ResponseStream<T> = Pin<Box<dyn Stream<Item = Result<T, ConnectError>> + Send>>;
type RequestStream<V> = Pin<Box<dyn Stream<Item = Result<OwnedView<V>, ConnectError>> + Send>>;

/// Trivial NumberService implementation. Each method demonstrates one
/// of the four RPC patterns.
struct NumberServiceImpl;

impl NumberService for NumberServiceImpl {
    /// Unary: square the input value.
    async fn square(
        &self,
        ctx: Context,
        request: OwnedView<SquareRequestView<'static>>,
    ) -> Result<(SquareResponse, Context), ConnectError> {
        // Edition 2023 default presence is EXPLICIT, so scalar fields
        // are Option<T>. unwrap_or(0) treats unset as zero, mirroring
        // the proto3 implicit-presence semantics.
        let v = request.value.unwrap_or(0) as i64;
        Ok((
            SquareResponse {
                squared: Some(v * v),
                ..Default::default()
            },
            ctx,
        ))
    }

    /// Server streaming: emit `count` consecutive integers from `start`.
    async fn range(
        &self,
        ctx: Context,
        request: OwnedView<RangeRequestView<'static>>,
    ) -> Result<(ResponseStream<RangeResponse>, Context), ConnectError> {
        let start = request.start.unwrap_or(0);
        let count = request.count.unwrap_or(0).max(0);
        let stream = futures::stream::iter((0..count).map(move |i| {
            Ok(RangeResponse {
                value: Some(start + i),
                ..Default::default()
            })
        }));
        Ok((Box::pin(stream), ctx))
    }

    /// Client streaming: drain the request stream, return the total.
    async fn sum(
        &self,
        ctx: Context,
        mut requests: RequestStream<SumRequestView<'static>>,
    ) -> Result<(SumResponse, Context), ConnectError> {
        let mut total: i64 = 0;
        while let Some(req) = requests.next().await {
            total += req?.value.unwrap_or(0) as i64;
        }
        Ok((
            SumResponse {
                total: Some(total),
                ..Default::default()
            },
            ctx,
        ))
    }

    /// Bidirectional streaming: emit a running total after each request.
    async fn running_sum(
        &self,
        ctx: Context,
        requests: RequestStream<RunningSumRequestView<'static>>,
    ) -> Result<(ResponseStream<RunningSumResponse>, Context), ConnectError> {
        let response_stream =
            futures::stream::unfold((requests, 0i64), |(mut requests, mut total)| async move {
                match requests.next().await? {
                    Ok(req) => {
                        total += req.value.unwrap_or(0) as i64;
                        Some((
                            Ok(RunningSumResponse {
                                total: Some(total),
                                ..Default::default()
                            }),
                            (requests, total),
                        ))
                    }
                    Err(e) => Some((Err(e), (requests, total))),
                }
            });
        Ok((Box::pin(response_stream), ctx))
    }
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let addr: std::net::SocketAddr = "127.0.0.1:8080".parse()?;

    let service = Arc::new(NumberServiceImpl);
    let router = service.register(Router::new());
    let app = router.into_axum_router();

    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("NumberService listening on http://{addr}");

    axum::serve(listener, app).await?;
    Ok(())
}
