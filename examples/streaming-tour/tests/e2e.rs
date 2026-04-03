//! End-to-end test: spin up the NumberService in-process, exercise all
//! four RPC types over a real TCP socket, assert expected results.

use std::pin::Pin;
use std::sync::Arc;

use buffa::view::OwnedView;
use connectrpc::client::{ClientConfig, HttpClient};
use connectrpc::{ConnectError, Context, Router};
use futures::{Stream, StreamExt};

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/_connectrpc.rs"));
}

use proto::anthropic::connectrpc::tour::v1::*;

// Local type aliases that flatten the streaming-handler signatures.
// See src/server.rs for the rationale.
type ResponseStream<T> = Pin<Box<dyn Stream<Item = Result<T, ConnectError>> + Send>>;
type RequestStream<V> = Pin<Box<dyn Stream<Item = Result<OwnedView<V>, ConnectError>> + Send>>;

struct NumberServiceImpl;

impl NumberService for NumberServiceImpl {
    async fn square(
        &self,
        ctx: Context,
        request: OwnedView<SquareRequestView<'static>>,
    ) -> Result<(SquareResponse, Context), ConnectError> {
        let v = request.value.unwrap_or(0) as i64;
        Ok((
            SquareResponse {
                squared: Some(v * v),
                ..Default::default()
            },
            ctx,
        ))
    }

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

async fn start_server() -> std::net::SocketAddr {
    let service = Arc::new(NumberServiceImpl);
    let router = service.register(Router::new());
    let app = router.into_axum_router();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

fn make_client(addr: std::net::SocketAddr) -> NumberServiceClient<HttpClient> {
    let config = ClientConfig::new(format!("http://{addr}").parse().unwrap());
    NumberServiceClient::new(HttpClient::plaintext(), config)
}

#[tokio::test]
async fn unary_square() {
    let addr = start_server().await;
    let client = make_client(addr);
    let resp = client
        .square(SquareRequest {
            value: Some(7),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(resp.view().squared, Some(49));
}

#[tokio::test]
async fn server_stream_range() {
    let addr = start_server().await;
    let client = make_client(addr);
    let mut stream = client
        .range(RangeRequest {
            start: Some(10),
            count: Some(5),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut got = Vec::new();
    while let Some(msg) = stream.message().await.unwrap() {
        got.push(msg.value.unwrap());
    }
    assert_eq!(got, vec![10, 11, 12, 13, 14]);
}

#[tokio::test]
async fn client_stream_sum() {
    let addr = start_server().await;
    let client = make_client(addr);
    let messages: Vec<SumRequest> = [3, 5, 7, 9]
        .iter()
        .map(|&v| SumRequest {
            value: Some(v),
            ..Default::default()
        })
        .collect();
    let resp = client.sum(messages).await.unwrap();
    assert_eq!(resp.view().total, Some(24));
}

#[tokio::test]
async fn bidi_stream_running_sum() {
    let addr = start_server().await;
    let client = make_client(addr);
    let mut bidi = client.running_sum().await.unwrap();
    let mut got = Vec::new();
    for v in [2, 4, 6, 8] {
        bidi.send(RunningSumRequest {
            value: Some(v),
            ..Default::default()
        })
        .await
        .unwrap();
        let msg = bidi.message().await.unwrap().unwrap();
        got.push(msg.total.unwrap());
    }
    bidi.close_send();
    assert_eq!(got, vec![2, 6, 12, 20]);
}
