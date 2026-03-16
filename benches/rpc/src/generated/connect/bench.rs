use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use ::connectrpc::{
    Context, ConnectError, Router, Dispatcher, view_handler_fn,
    view_streaming_handler_fn, view_client_streaming_handler_fn,
    view_bidi_streaming_handler_fn,
};
use ::connectrpc::dispatcher::codegen as __crpc_codegen;
use ::connectrpc::CodecFormat as __CodecFormat;
use buffa::bytes::Bytes as __Bytes;
use ::connectrpc::client::{
    ClientConfig, ClientTransport, CallOptions, call_unary, call_server_stream,
    call_client_stream, call_bidi_stream,
};
use futures::Stream;
use buffa::Message;
use buffa::view::OwnedView;
/// Full service name for this service.
pub const BENCH_SERVICE_SERVICE_NAME: &str = "bench.v1.BenchService";
/// Server trait for BenchService.
///
/// # Implementing handlers
///
/// Handlers receive requests as `OwnedView<FooView<'static>>`, which gives
/// zero-copy borrowed access to fields (e.g. `request.name` is a `&str`
/// into the decoded buffer). The view can be held across `.await` points.
///
/// Implement methods with plain `async fn`; the returned future satisfies
/// the `Send` bound automatically. See the
/// [buffa user guide](https://github.com/anthropics/buffa/blob/main/docs/guide.md#ownedview-in-async-trait-implementations)
/// for zero-copy access patterns and when `to_owned_message()` is needed.
#[allow(clippy::type_complexity)]
pub trait BenchService: Send + Sync + 'static {
    /// Handle the Unary RPC.
    fn unary(
        &self,
        ctx: Context,
        request: OwnedView<crate::proto::bench::v1::BenchRequestView<'static>>,
    ) -> impl Future<
        Output = Result<(crate::proto::bench::v1::BenchResponse, Context), ConnectError>,
    > + Send;
    /// Handle the ServerStream RPC.
    fn server_stream(
        &self,
        ctx: Context,
        request: OwnedView<crate::proto::bench::v1::BenchRequestView<'static>>,
    ) -> impl Future<
        Output = Result<
            (
                Pin<
                    Box<
                        dyn Stream<
                            Item = Result<
                                crate::proto::bench::v1::BenchResponse,
                                ConnectError,
                            >,
                        > + Send,
                    >,
                >,
                Context,
            ),
            ConnectError,
        >,
    > + Send;
    /// Handle the ClientStream RPC.
    fn client_stream(
        &self,
        ctx: Context,
        requests: Pin<
            Box<
                dyn Stream<
                    Item = Result<
                        OwnedView<crate::proto::bench::v1::BenchRequestView<'static>>,
                        ConnectError,
                    >,
                > + Send,
            >,
        >,
    ) -> impl Future<
        Output = Result<(crate::proto::bench::v1::BenchResponse, Context), ConnectError>,
    > + Send;
    /// Handle the BidiStream RPC.
    fn bidi_stream(
        &self,
        ctx: Context,
        requests: Pin<
            Box<
                dyn Stream<
                    Item = Result<
                        OwnedView<crate::proto::bench::v1::BenchRequestView<'static>>,
                        ConnectError,
                    >,
                > + Send,
            >,
        >,
    ) -> impl Future<
        Output = Result<
            (
                Pin<
                    Box<
                        dyn Stream<
                            Item = Result<
                                crate::proto::bench::v1::BenchResponse,
                                ConnectError,
                            >,
                        > + Send,
                    >,
                >,
                Context,
            ),
            ConnectError,
        >,
    > + Send;
    /// Handle the LogUnary RPC.
    fn log_unary(
        &self,
        ctx: Context,
        request: OwnedView<crate::proto::bench::v1::LogRequestView<'static>>,
    ) -> impl Future<
        Output = Result<(crate::proto::bench::v1::LogResponse, Context), ConnectError>,
    > + Send;
    /// Handle the LogUnaryOwned RPC.
    fn log_unary_owned(
        &self,
        ctx: Context,
        request: OwnedView<crate::proto::bench::v1::LogRequestView<'static>>,
    ) -> impl Future<
        Output = Result<(crate::proto::bench::v1::LogResponse, Context), ConnectError>,
    > + Send;
}
/// Extension trait for registering a service implementation with a Router.
///
/// This trait is automatically implemented for all types that implement the service trait.
///
/// # Example
///
/// ```rust,ignore
/// use std::sync::Arc;
///
/// let service = Arc::new(MyServiceImpl);
/// let router = service.register(Router::new());
/// ```
pub trait BenchServiceExt: BenchService {
    /// Register this service implementation with a Router.
    ///
    /// Takes ownership of the `Arc<Self>` and returns a new Router with
    /// this service's methods registered.
    fn register(self: Arc<Self>, router: Router) -> Router;
}
impl<S: BenchService> BenchServiceExt for S {
    fn register(self: Arc<Self>, router: Router) -> Router {
        router
            .route_view(
                BENCH_SERVICE_SERVICE_NAME,
                "Unary",
                {
                    let svc = Arc::clone(&self);
                    view_handler_fn(move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.unary(ctx, req).await }
                    })
                },
            )
            .route_view_server_stream(
                BENCH_SERVICE_SERVICE_NAME,
                "ServerStream",
                view_streaming_handler_fn({
                    let svc = Arc::clone(&self);
                    move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.server_stream(ctx, req).await }
                    }
                }),
            )
            .route_view_client_stream(
                BENCH_SERVICE_SERVICE_NAME,
                "ClientStream",
                view_client_streaming_handler_fn({
                    let svc = Arc::clone(&self);
                    move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.client_stream(ctx, req).await }
                    }
                }),
            )
            .route_view_bidi_stream(
                BENCH_SERVICE_SERVICE_NAME,
                "BidiStream",
                view_bidi_streaming_handler_fn({
                    let svc = Arc::clone(&self);
                    move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.bidi_stream(ctx, req).await }
                    }
                }),
            )
            .route_view(
                BENCH_SERVICE_SERVICE_NAME,
                "LogUnary",
                {
                    let svc = Arc::clone(&self);
                    view_handler_fn(move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.log_unary(ctx, req).await }
                    })
                },
            )
            .route_view(
                BENCH_SERVICE_SERVICE_NAME,
                "LogUnaryOwned",
                {
                    let svc = Arc::clone(&self);
                    view_handler_fn(move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.log_unary_owned(ctx, req).await }
                    })
                },
            )
    }
}
/// Monomorphic dispatcher for `BenchService`.
///
/// Unlike `.register(Router)` which type-erases each method into an `Arc<dyn ErasedHandler>` stored in a `HashMap`, this struct dispatches via a compile-time `match` on method name: no vtable, no hash lookup.
///
/// # Example
///
/// ```rust,ignore
/// use connectrpc::ConnectRpcService;
///
/// let server = BenchServiceServer::new(MyImpl);
/// let service = ConnectRpcService::new(server);
/// // hand `service` to axum/hyper as a fallback_service
/// ```
pub struct BenchServiceServer<T> {
    inner: Arc<T>,
}
impl<T: BenchService> BenchServiceServer<T> {
    /// Wrap a service implementation in a monomorphic dispatcher.
    pub fn new(service: T) -> Self {
        Self { inner: Arc::new(service) }
    }
    /// Wrap an already-`Arc`'d service implementation.
    pub fn from_arc(inner: Arc<T>) -> Self {
        Self { inner }
    }
}
impl<T> Clone for BenchServiceServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
impl<T: BenchService> Dispatcher for BenchServiceServer<T> {
    #[inline]
    fn lookup(&self, path: &str) -> Option<__crpc_codegen::MethodDescriptor> {
        let method = path.strip_prefix("bench.v1.BenchService/")?;
        match method {
            "Unary" => Some(__crpc_codegen::MethodDescriptor::unary(false)),
            "ServerStream" => Some(__crpc_codegen::MethodDescriptor::server_streaming()),
            "ClientStream" => Some(__crpc_codegen::MethodDescriptor::client_streaming()),
            "BidiStream" => Some(__crpc_codegen::MethodDescriptor::bidi_streaming()),
            "LogUnary" => Some(__crpc_codegen::MethodDescriptor::unary(false)),
            "LogUnaryOwned" => Some(__crpc_codegen::MethodDescriptor::unary(false)),
            _ => None,
        }
    }
    fn call_unary(
        &self,
        path: &str,
        ctx: Context,
        request: __Bytes,
        format: __CodecFormat,
    ) -> __crpc_codegen::UnaryResult {
        let Some(method) = path.strip_prefix("bench.v1.BenchService/") else {
            return __crpc_codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "Unary" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = __crpc_codegen::decode_request_view::<
                        crate::proto::bench::v1::BenchRequestView,
                    >(request, format)?;
                    let (res, ctx) = svc.unary(ctx, req).await?;
                    let bytes = __crpc_codegen::encode_response(&res, format)?;
                    Ok((bytes, ctx))
                })
            }
            "LogUnary" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = __crpc_codegen::decode_request_view::<
                        crate::proto::bench::v1::LogRequestView,
                    >(request, format)?;
                    let (res, ctx) = svc.log_unary(ctx, req).await?;
                    let bytes = __crpc_codegen::encode_response(&res, format)?;
                    Ok((bytes, ctx))
                })
            }
            "LogUnaryOwned" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = __crpc_codegen::decode_request_view::<
                        crate::proto::bench::v1::LogRequestView,
                    >(request, format)?;
                    let (res, ctx) = svc.log_unary_owned(ctx, req).await?;
                    let bytes = __crpc_codegen::encode_response(&res, format)?;
                    Ok((bytes, ctx))
                })
            }
            _ => __crpc_codegen::unimplemented_unary(path),
        }
    }
    fn call_server_streaming(
        &self,
        path: &str,
        ctx: Context,
        request: __Bytes,
        format: __CodecFormat,
    ) -> __crpc_codegen::StreamingResult {
        let Some(method) = path.strip_prefix("bench.v1.BenchService/") else {
            return __crpc_codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "ServerStream" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = __crpc_codegen::decode_request_view::<
                        crate::proto::bench::v1::BenchRequestView,
                    >(request, format)?;
                    let (resp_stream, ctx) = svc.server_stream(ctx, req).await?;
                    Ok((
                        __crpc_codegen::encode_response_stream(resp_stream, format),
                        ctx,
                    ))
                })
            }
            _ => __crpc_codegen::unimplemented_streaming(path),
        }
    }
    fn call_client_streaming(
        &self,
        path: &str,
        ctx: Context,
        requests: __crpc_codegen::RequestStream,
        format: __CodecFormat,
    ) -> __crpc_codegen::UnaryResult {
        let Some(method) = path.strip_prefix("bench.v1.BenchService/") else {
            return __crpc_codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &requests, &format);
        match method {
            "ClientStream" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req_stream = __crpc_codegen::decode_view_request_stream::<
                        crate::proto::bench::v1::BenchRequestView,
                    >(requests, format);
                    let (res, ctx) = svc.client_stream(ctx, req_stream).await?;
                    let bytes = __crpc_codegen::encode_response(&res, format)?;
                    Ok((bytes, ctx))
                })
            }
            _ => __crpc_codegen::unimplemented_unary(path),
        }
    }
    fn call_bidi_streaming(
        &self,
        path: &str,
        ctx: Context,
        requests: __crpc_codegen::RequestStream,
        format: __CodecFormat,
    ) -> __crpc_codegen::StreamingResult {
        let Some(method) = path.strip_prefix("bench.v1.BenchService/") else {
            return __crpc_codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &requests, &format);
        match method {
            "BidiStream" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req_stream = __crpc_codegen::decode_view_request_stream::<
                        crate::proto::bench::v1::BenchRequestView,
                    >(requests, format);
                    let (resp_stream, ctx) = svc.bidi_stream(ctx, req_stream).await?;
                    Ok((
                        __crpc_codegen::encode_response_stream(resp_stream, format),
                        ctx,
                    ))
                })
            }
            _ => __crpc_codegen::unimplemented_streaming(path),
        }
    }
}
/// Client for this service.
///
/// Generic over `T: ClientTransport`. For **gRPC** (HTTP/2), use
/// `Http2Connection` — it has honest `poll_ready` and composes with
/// `tower::balance` for multi-connection load balancing. For **Connect
/// over HTTP/1.1** (or unknown protocol), use `HttpClient`.
///
/// # Example (gRPC / HTTP/2)
///
/// ```rust,ignore
/// use connectrpc::client::{Http2Connection, ClientConfig};
/// use connectrpc::Protocol;
///
/// let uri: http::Uri = "http://localhost:8080".parse()?;
/// let conn = Http2Connection::connect_plaintext(uri.clone()).await?.shared(1024);
/// let config = ClientConfig::new(uri).protocol(Protocol::Grpc);
///
/// let client = BenchServiceClient::new(conn, config);
/// let response = client.unary(request).await?;
/// ```
///
/// # Example (Connect / HTTP/1.1 or ALPN)
///
/// ```rust,ignore
/// use connectrpc::client::{HttpClient, ClientConfig};
///
/// let http = HttpClient::plaintext();  // cleartext http:// only
/// let config = ClientConfig::new("http://localhost:8080".parse()?);
///
/// let client = BenchServiceClient::new(http, config);
/// let response = client.unary(request).await?;
/// ```
///
/// # Working with the response
///
/// Unary calls return [`UnaryResponse<OwnedView<FooView>>`](::connectrpc::client::UnaryResponse).
/// The `OwnedView` derefs to the view, so field access is zero-copy:
///
/// ```rust,ignore
/// let resp = client.unary(request).await?.into_view();
/// let name: &str = resp.name;  // borrow into the response buffer
/// ```
///
/// If you need the owned struct (e.g. to store or pass by value), use
/// [`into_owned()`](::connectrpc::client::UnaryResponse::into_owned):
///
/// ```rust,ignore
/// let owned = client.unary(request).await?.into_owned();
/// ```
#[derive(Clone)]
pub struct BenchServiceClient<T> {
    transport: T,
    config: ClientConfig,
}
impl<T> BenchServiceClient<T>
where
    T: ClientTransport,
    <T::ResponseBody as http_body::Body>::Error: std::fmt::Display,
{
    /// Create a new client with the given transport and configuration.
    pub fn new(transport: T, config: ClientConfig) -> Self {
        Self { transport, config }
    }
    /// Get the client configuration.
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }
    /// Get a mutable reference to the client configuration.
    pub fn config_mut(&mut self) -> &mut ClientConfig {
        &mut self.config
    }
    /// Call the Unary RPC. Sends a request to /bench.v1.BenchService/Unary.
    pub async fn unary(
        &self,
        request: crate::proto::bench::v1::BenchRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<crate::proto::bench::v1::BenchResponseView<'static>>,
        >,
        ConnectError,
    > {
        self.unary_with_options(request, CallOptions::default()).await
    }
    /// Call the Unary RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn unary_with_options(
        &self,
        request: crate::proto::bench::v1::BenchRequest,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<crate::proto::bench::v1::BenchResponseView<'static>>,
        >,
        ConnectError,
    > {
        call_unary(
                &self.transport,
                &self.config,
                "bench.v1.BenchService",
                "Unary",
                request,
                options,
            )
            .await
    }
    /// Call the ServerStream RPC. Sends a request to /bench.v1.BenchService/ServerStream.
    pub async fn server_stream(
        &self,
        request: crate::proto::bench::v1::BenchRequest,
    ) -> Result<
        ::connectrpc::client::ServerStream<
            T::ResponseBody,
            crate::proto::bench::v1::BenchResponseView<'static>,
        >,
        ConnectError,
    > {
        self.server_stream_with_options(request, CallOptions::default()).await
    }
    /// Call the ServerStream RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn server_stream_with_options(
        &self,
        request: crate::proto::bench::v1::BenchRequest,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::ServerStream<
            T::ResponseBody,
            crate::proto::bench::v1::BenchResponseView<'static>,
        >,
        ConnectError,
    > {
        call_server_stream(
                &self.transport,
                &self.config,
                "bench.v1.BenchService",
                "ServerStream",
                request,
                options,
            )
            .await
    }
    /// Call the ClientStream RPC. Sends a request to /bench.v1.BenchService/ClientStream.
    pub async fn client_stream(
        &self,
        requests: impl IntoIterator<Item = crate::proto::bench::v1::BenchRequest>,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<crate::proto::bench::v1::BenchResponseView<'static>>,
        >,
        ConnectError,
    > {
        self.client_stream_with_options(requests, CallOptions::default()).await
    }
    /// Call the ClientStream RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn client_stream_with_options(
        &self,
        requests: impl IntoIterator<Item = crate::proto::bench::v1::BenchRequest>,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<crate::proto::bench::v1::BenchResponseView<'static>>,
        >,
        ConnectError,
    > {
        call_client_stream(
                &self.transport,
                &self.config,
                "bench.v1.BenchService",
                "ClientStream",
                requests,
                options,
            )
            .await
    }
    /// Call the BidiStream RPC. Sends a request to /bench.v1.BenchService/BidiStream.
    pub async fn bidi_stream(
        &self,
    ) -> Result<
        ::connectrpc::client::BidiStream<
            T::ResponseBody,
            crate::proto::bench::v1::BenchRequest,
            crate::proto::bench::v1::BenchResponseView<'static>,
        >,
        ConnectError,
    > {
        self.bidi_stream_with_options(CallOptions::default()).await
    }
    /// Call the BidiStream RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn bidi_stream_with_options(
        &self,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::BidiStream<
            T::ResponseBody,
            crate::proto::bench::v1::BenchRequest,
            crate::proto::bench::v1::BenchResponseView<'static>,
        >,
        ConnectError,
    > {
        call_bidi_stream(
                &self.transport,
                &self.config,
                "bench.v1.BenchService",
                "BidiStream",
                options,
            )
            .await
    }
    /// Call the LogUnary RPC. Sends a request to /bench.v1.BenchService/LogUnary.
    pub async fn log_unary(
        &self,
        request: crate::proto::bench::v1::LogRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<crate::proto::bench::v1::LogResponseView<'static>>,
        >,
        ConnectError,
    > {
        self.log_unary_with_options(request, CallOptions::default()).await
    }
    /// Call the LogUnary RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn log_unary_with_options(
        &self,
        request: crate::proto::bench::v1::LogRequest,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<crate::proto::bench::v1::LogResponseView<'static>>,
        >,
        ConnectError,
    > {
        call_unary(
                &self.transport,
                &self.config,
                "bench.v1.BenchService",
                "LogUnary",
                request,
                options,
            )
            .await
    }
    /// Call the LogUnaryOwned RPC. Sends a request to /bench.v1.BenchService/LogUnaryOwned.
    pub async fn log_unary_owned(
        &self,
        request: crate::proto::bench::v1::LogRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<crate::proto::bench::v1::LogResponseView<'static>>,
        >,
        ConnectError,
    > {
        self.log_unary_owned_with_options(request, CallOptions::default()).await
    }
    /// Call the LogUnaryOwned RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn log_unary_owned_with_options(
        &self,
        request: crate::proto::bench::v1::LogRequest,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<crate::proto::bench::v1::LogResponseView<'static>>,
        >,
        ConnectError,
    > {
        call_unary(
                &self.transport,
                &self.config,
                "bench.v1.BenchService",
                "LogUnaryOwned",
                request,
                options,
            )
            .await
    }
}
/// Full service name for this service.
pub const ECHO_SERVICE_SERVICE_NAME: &str = "bench.v1.EchoService";
/// Minimal echo service for measuring pure framework overhead.
/// No database, no spawn_blocking, no complex payloads — just
/// dispatch + proto encode/decode of a single string.
///
/// # Implementing handlers
///
/// Handlers receive requests as `OwnedView<FooView<'static>>`, which gives
/// zero-copy borrowed access to fields (e.g. `request.name` is a `&str`
/// into the decoded buffer). The view can be held across `.await` points.
///
/// Implement methods with plain `async fn`; the returned future satisfies
/// the `Send` bound automatically. See the
/// [buffa user guide](https://github.com/anthropics/buffa/blob/main/docs/guide.md#ownedview-in-async-trait-implementations)
/// for zero-copy access patterns and when `to_owned_message()` is needed.
#[allow(clippy::type_complexity)]
pub trait EchoService: Send + Sync + 'static {
    /// Handle the Echo RPC.
    fn echo(
        &self,
        ctx: Context,
        request: OwnedView<crate::proto::bench::v1::EchoRequestView<'static>>,
    ) -> impl Future<
        Output = Result<(crate::proto::bench::v1::EchoResponse, Context), ConnectError>,
    > + Send;
}
/// Extension trait for registering a service implementation with a Router.
///
/// This trait is automatically implemented for all types that implement the service trait.
///
/// # Example
///
/// ```rust,ignore
/// use std::sync::Arc;
///
/// let service = Arc::new(MyServiceImpl);
/// let router = service.register(Router::new());
/// ```
pub trait EchoServiceExt: EchoService {
    /// Register this service implementation with a Router.
    ///
    /// Takes ownership of the `Arc<Self>` and returns a new Router with
    /// this service's methods registered.
    fn register(self: Arc<Self>, router: Router) -> Router;
}
impl<S: EchoService> EchoServiceExt for S {
    fn register(self: Arc<Self>, router: Router) -> Router {
        router
            .route_view(
                ECHO_SERVICE_SERVICE_NAME,
                "Echo",
                {
                    let svc = Arc::clone(&self);
                    view_handler_fn(move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.echo(ctx, req).await }
                    })
                },
            )
    }
}
/// Monomorphic dispatcher for `EchoService`.
///
/// Unlike `.register(Router)` which type-erases each method into an `Arc<dyn ErasedHandler>` stored in a `HashMap`, this struct dispatches via a compile-time `match` on method name: no vtable, no hash lookup.
///
/// # Example
///
/// ```rust,ignore
/// use connectrpc::ConnectRpcService;
///
/// let server = EchoServiceServer::new(MyImpl);
/// let service = ConnectRpcService::new(server);
/// // hand `service` to axum/hyper as a fallback_service
/// ```
pub struct EchoServiceServer<T> {
    inner: Arc<T>,
}
impl<T: EchoService> EchoServiceServer<T> {
    /// Wrap a service implementation in a monomorphic dispatcher.
    pub fn new(service: T) -> Self {
        Self { inner: Arc::new(service) }
    }
    /// Wrap an already-`Arc`'d service implementation.
    pub fn from_arc(inner: Arc<T>) -> Self {
        Self { inner }
    }
}
impl<T> Clone for EchoServiceServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
impl<T: EchoService> Dispatcher for EchoServiceServer<T> {
    #[inline]
    fn lookup(&self, path: &str) -> Option<__crpc_codegen::MethodDescriptor> {
        let method = path.strip_prefix("bench.v1.EchoService/")?;
        match method {
            "Echo" => Some(__crpc_codegen::MethodDescriptor::unary(false)),
            _ => None,
        }
    }
    fn call_unary(
        &self,
        path: &str,
        ctx: Context,
        request: __Bytes,
        format: __CodecFormat,
    ) -> __crpc_codegen::UnaryResult {
        let Some(method) = path.strip_prefix("bench.v1.EchoService/") else {
            return __crpc_codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "Echo" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = __crpc_codegen::decode_request_view::<
                        crate::proto::bench::v1::EchoRequestView,
                    >(request, format)?;
                    let (res, ctx) = svc.echo(ctx, req).await?;
                    let bytes = __crpc_codegen::encode_response(&res, format)?;
                    Ok((bytes, ctx))
                })
            }
            _ => __crpc_codegen::unimplemented_unary(path),
        }
    }
    fn call_server_streaming(
        &self,
        path: &str,
        ctx: Context,
        request: __Bytes,
        format: __CodecFormat,
    ) -> __crpc_codegen::StreamingResult {
        let Some(method) = path.strip_prefix("bench.v1.EchoService/") else {
            return __crpc_codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            _ => __crpc_codegen::unimplemented_streaming(path),
        }
    }
    fn call_client_streaming(
        &self,
        path: &str,
        ctx: Context,
        requests: __crpc_codegen::RequestStream,
        format: __CodecFormat,
    ) -> __crpc_codegen::UnaryResult {
        let Some(method) = path.strip_prefix("bench.v1.EchoService/") else {
            return __crpc_codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &requests, &format);
        match method {
            _ => __crpc_codegen::unimplemented_unary(path),
        }
    }
    fn call_bidi_streaming(
        &self,
        path: &str,
        ctx: Context,
        requests: __crpc_codegen::RequestStream,
        format: __CodecFormat,
    ) -> __crpc_codegen::StreamingResult {
        let Some(method) = path.strip_prefix("bench.v1.EchoService/") else {
            return __crpc_codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &requests, &format);
        match method {
            _ => __crpc_codegen::unimplemented_streaming(path),
        }
    }
}
/// Client for this service.
///
/// Generic over `T: ClientTransport`. For **gRPC** (HTTP/2), use
/// `Http2Connection` — it has honest `poll_ready` and composes with
/// `tower::balance` for multi-connection load balancing. For **Connect
/// over HTTP/1.1** (or unknown protocol), use `HttpClient`.
///
/// # Example (gRPC / HTTP/2)
///
/// ```rust,ignore
/// use connectrpc::client::{Http2Connection, ClientConfig};
/// use connectrpc::Protocol;
///
/// let uri: http::Uri = "http://localhost:8080".parse()?;
/// let conn = Http2Connection::connect_plaintext(uri.clone()).await?.shared(1024);
/// let config = ClientConfig::new(uri).protocol(Protocol::Grpc);
///
/// let client = EchoServiceClient::new(conn, config);
/// let response = client.echo(request).await?;
/// ```
///
/// # Example (Connect / HTTP/1.1 or ALPN)
///
/// ```rust,ignore
/// use connectrpc::client::{HttpClient, ClientConfig};
///
/// let http = HttpClient::plaintext();  // cleartext http:// only
/// let config = ClientConfig::new("http://localhost:8080".parse()?);
///
/// let client = EchoServiceClient::new(http, config);
/// let response = client.echo(request).await?;
/// ```
///
/// # Working with the response
///
/// Unary calls return [`UnaryResponse<OwnedView<FooView>>`](::connectrpc::client::UnaryResponse).
/// The `OwnedView` derefs to the view, so field access is zero-copy:
///
/// ```rust,ignore
/// let resp = client.echo(request).await?.into_view();
/// let name: &str = resp.name;  // borrow into the response buffer
/// ```
///
/// If you need the owned struct (e.g. to store or pass by value), use
/// [`into_owned()`](::connectrpc::client::UnaryResponse::into_owned):
///
/// ```rust,ignore
/// let owned = client.echo(request).await?.into_owned();
/// ```
#[derive(Clone)]
pub struct EchoServiceClient<T> {
    transport: T,
    config: ClientConfig,
}
impl<T> EchoServiceClient<T>
where
    T: ClientTransport,
    <T::ResponseBody as http_body::Body>::Error: std::fmt::Display,
{
    /// Create a new client with the given transport and configuration.
    pub fn new(transport: T, config: ClientConfig) -> Self {
        Self { transport, config }
    }
    /// Get the client configuration.
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }
    /// Get a mutable reference to the client configuration.
    pub fn config_mut(&mut self) -> &mut ClientConfig {
        &mut self.config
    }
    /// Call the Echo RPC. Sends a request to /bench.v1.EchoService/Echo.
    pub async fn echo(
        &self,
        request: crate::proto::bench::v1::EchoRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<crate::proto::bench::v1::EchoResponseView<'static>>,
        >,
        ConnectError,
    > {
        self.echo_with_options(request, CallOptions::default()).await
    }
    /// Call the Echo RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn echo_with_options(
        &self,
        request: crate::proto::bench::v1::EchoRequest,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<crate::proto::bench::v1::EchoResponseView<'static>>,
        >,
        ConnectError,
    > {
        call_unary(
                &self.transport,
                &self.config,
                "bench.v1.EchoService",
                "Echo",
                request,
                options,
            )
            .await
    }
}
/// Full service name for this service.
pub const LOG_INGEST_SERVICE_SERVICE_NAME: &str = "bench.v1.LogIngestService";
/// Server trait for LogIngestService.
///
/// # Implementing handlers
///
/// Handlers receive requests as `OwnedView<FooView<'static>>`, which gives
/// zero-copy borrowed access to fields (e.g. `request.name` is a `&str`
/// into the decoded buffer). The view can be held across `.await` points.
///
/// Implement methods with plain `async fn`; the returned future satisfies
/// the `Send` bound automatically. See the
/// [buffa user guide](https://github.com/anthropics/buffa/blob/main/docs/guide.md#ownedview-in-async-trait-implementations)
/// for zero-copy access patterns and when `to_owned_message()` is needed.
#[allow(clippy::type_complexity)]
pub trait LogIngestService: Send + Sync + 'static {
    /// Handle the Ingest RPC.
    fn ingest(
        &self,
        ctx: Context,
        request: OwnedView<crate::proto::bench::v1::LogRequestView<'static>>,
    ) -> impl Future<
        Output = Result<
            (crate::proto::bench::v1::LogIngestResponse, Context),
            ConnectError,
        >,
    > + Send;
}
/// Extension trait for registering a service implementation with a Router.
///
/// This trait is automatically implemented for all types that implement the service trait.
///
/// # Example
///
/// ```rust,ignore
/// use std::sync::Arc;
///
/// let service = Arc::new(MyServiceImpl);
/// let router = service.register(Router::new());
/// ```
pub trait LogIngestServiceExt: LogIngestService {
    /// Register this service implementation with a Router.
    ///
    /// Takes ownership of the `Arc<Self>` and returns a new Router with
    /// this service's methods registered.
    fn register(self: Arc<Self>, router: Router) -> Router;
}
impl<S: LogIngestService> LogIngestServiceExt for S {
    fn register(self: Arc<Self>, router: Router) -> Router {
        router
            .route_view(
                LOG_INGEST_SERVICE_SERVICE_NAME,
                "Ingest",
                {
                    let svc = Arc::clone(&self);
                    view_handler_fn(move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.ingest(ctx, req).await }
                    })
                },
            )
    }
}
/// Monomorphic dispatcher for `LogIngestService`.
///
/// Unlike `.register(Router)` which type-erases each method into an `Arc<dyn ErasedHandler>` stored in a `HashMap`, this struct dispatches via a compile-time `match` on method name: no vtable, no hash lookup.
///
/// # Example
///
/// ```rust,ignore
/// use connectrpc::ConnectRpcService;
///
/// let server = LogIngestServiceServer::new(MyImpl);
/// let service = ConnectRpcService::new(server);
/// // hand `service` to axum/hyper as a fallback_service
/// ```
pub struct LogIngestServiceServer<T> {
    inner: Arc<T>,
}
impl<T: LogIngestService> LogIngestServiceServer<T> {
    /// Wrap a service implementation in a monomorphic dispatcher.
    pub fn new(service: T) -> Self {
        Self { inner: Arc::new(service) }
    }
    /// Wrap an already-`Arc`'d service implementation.
    pub fn from_arc(inner: Arc<T>) -> Self {
        Self { inner }
    }
}
impl<T> Clone for LogIngestServiceServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
impl<T: LogIngestService> Dispatcher for LogIngestServiceServer<T> {
    #[inline]
    fn lookup(&self, path: &str) -> Option<__crpc_codegen::MethodDescriptor> {
        let method = path.strip_prefix("bench.v1.LogIngestService/")?;
        match method {
            "Ingest" => Some(__crpc_codegen::MethodDescriptor::unary(false)),
            _ => None,
        }
    }
    fn call_unary(
        &self,
        path: &str,
        ctx: Context,
        request: __Bytes,
        format: __CodecFormat,
    ) -> __crpc_codegen::UnaryResult {
        let Some(method) = path.strip_prefix("bench.v1.LogIngestService/") else {
            return __crpc_codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "Ingest" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = __crpc_codegen::decode_request_view::<
                        crate::proto::bench::v1::LogRequestView,
                    >(request, format)?;
                    let (res, ctx) = svc.ingest(ctx, req).await?;
                    let bytes = __crpc_codegen::encode_response(&res, format)?;
                    Ok((bytes, ctx))
                })
            }
            _ => __crpc_codegen::unimplemented_unary(path),
        }
    }
    fn call_server_streaming(
        &self,
        path: &str,
        ctx: Context,
        request: __Bytes,
        format: __CodecFormat,
    ) -> __crpc_codegen::StreamingResult {
        let Some(method) = path.strip_prefix("bench.v1.LogIngestService/") else {
            return __crpc_codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            _ => __crpc_codegen::unimplemented_streaming(path),
        }
    }
    fn call_client_streaming(
        &self,
        path: &str,
        ctx: Context,
        requests: __crpc_codegen::RequestStream,
        format: __CodecFormat,
    ) -> __crpc_codegen::UnaryResult {
        let Some(method) = path.strip_prefix("bench.v1.LogIngestService/") else {
            return __crpc_codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &requests, &format);
        match method {
            _ => __crpc_codegen::unimplemented_unary(path),
        }
    }
    fn call_bidi_streaming(
        &self,
        path: &str,
        ctx: Context,
        requests: __crpc_codegen::RequestStream,
        format: __CodecFormat,
    ) -> __crpc_codegen::StreamingResult {
        let Some(method) = path.strip_prefix("bench.v1.LogIngestService/") else {
            return __crpc_codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &requests, &format);
        match method {
            _ => __crpc_codegen::unimplemented_streaming(path),
        }
    }
}
/// Client for this service.
///
/// Generic over `T: ClientTransport`. For **gRPC** (HTTP/2), use
/// `Http2Connection` — it has honest `poll_ready` and composes with
/// `tower::balance` for multi-connection load balancing. For **Connect
/// over HTTP/1.1** (or unknown protocol), use `HttpClient`.
///
/// # Example (gRPC / HTTP/2)
///
/// ```rust,ignore
/// use connectrpc::client::{Http2Connection, ClientConfig};
/// use connectrpc::Protocol;
///
/// let uri: http::Uri = "http://localhost:8080".parse()?;
/// let conn = Http2Connection::connect_plaintext(uri.clone()).await?.shared(1024);
/// let config = ClientConfig::new(uri).protocol(Protocol::Grpc);
///
/// let client = LogIngestServiceClient::new(conn, config);
/// let response = client.ingest(request).await?;
/// ```
///
/// # Example (Connect / HTTP/1.1 or ALPN)
///
/// ```rust,ignore
/// use connectrpc::client::{HttpClient, ClientConfig};
///
/// let http = HttpClient::plaintext();  // cleartext http:// only
/// let config = ClientConfig::new("http://localhost:8080".parse()?);
///
/// let client = LogIngestServiceClient::new(http, config);
/// let response = client.ingest(request).await?;
/// ```
///
/// # Working with the response
///
/// Unary calls return [`UnaryResponse<OwnedView<FooView>>`](::connectrpc::client::UnaryResponse).
/// The `OwnedView` derefs to the view, so field access is zero-copy:
///
/// ```rust,ignore
/// let resp = client.ingest(request).await?.into_view();
/// let name: &str = resp.name;  // borrow into the response buffer
/// ```
///
/// If you need the owned struct (e.g. to store or pass by value), use
/// [`into_owned()`](::connectrpc::client::UnaryResponse::into_owned):
///
/// ```rust,ignore
/// let owned = client.ingest(request).await?.into_owned();
/// ```
#[derive(Clone)]
pub struct LogIngestServiceClient<T> {
    transport: T,
    config: ClientConfig,
}
impl<T> LogIngestServiceClient<T>
where
    T: ClientTransport,
    <T::ResponseBody as http_body::Body>::Error: std::fmt::Display,
{
    /// Create a new client with the given transport and configuration.
    pub fn new(transport: T, config: ClientConfig) -> Self {
        Self { transport, config }
    }
    /// Get the client configuration.
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }
    /// Get a mutable reference to the client configuration.
    pub fn config_mut(&mut self) -> &mut ClientConfig {
        &mut self.config
    }
    /// Call the Ingest RPC. Sends a request to /bench.v1.LogIngestService/Ingest.
    pub async fn ingest(
        &self,
        request: crate::proto::bench::v1::LogRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<crate::proto::bench::v1::LogIngestResponseView<'static>>,
        >,
        ConnectError,
    > {
        self.ingest_with_options(request, CallOptions::default()).await
    }
    /// Call the Ingest RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn ingest_with_options(
        &self,
        request: crate::proto::bench::v1::LogRequest,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<crate::proto::bench::v1::LogIngestResponseView<'static>>,
        >,
        ConnectError,
    > {
        call_unary(
                &self.transport,
                &self.config,
                "bench.v1.LogIngestService",
                "Ingest",
                request,
                options,
            )
            .await
    }
}
