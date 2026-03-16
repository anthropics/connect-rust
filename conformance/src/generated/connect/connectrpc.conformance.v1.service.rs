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
pub const CONFORMANCE_SERVICE_SERVICE_NAME: &str = "connectrpc.conformance.v1.ConformanceService";
/// The service implemented by conformance test servers. This is implemented by
/// the reference servers, used to test clients, and is expected to be implemented
/// by test servers, since this is the service used by reference clients.
/// Test servers must implement the service as described.
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
pub trait ConformanceService: Send + Sync + 'static {
    /// A unary operation. The request indicates the response headers and trailers
    /// and also indicates either a response message or an error to send back.
    /// Response message data is specified as bytes. The service should echo back
    /// request properties in the ConformancePayload and then include the message
    /// data in the data field.
    /// If the response_delay_ms duration is specified, the server should wait the
    /// given duration after reading the request before sending the corresponding
    /// response.
    /// Servers should allow the response definition to be unset in the request and
    /// if it is, set no response headers or trailers and return no response data.
    /// The returned payload should only contain the request info.
    fn unary(
        &self,
        ctx: Context,
        request: OwnedView<
            crate::proto::connectrpc::conformance::v1::UnaryRequestView<'static>,
        >,
    ) -> impl Future<
        Output = Result<
            (crate::proto::connectrpc::conformance::v1::UnaryResponse, Context),
            ConnectError,
        >,
    > + Send;
    /// A server-streaming operation. The request indicates the response headers,
    /// response messages, trailers, and an optional error to send back. The
    /// response data should be sent in the order indicated, and the server should
    /// wait between sending response messages as indicated.
    /// Response message data is specified as bytes. The service should echo back
    /// request properties in the first ConformancePayload, and then include the
    /// message data in the data field. Subsequent messages after the first one
    /// should contain only the data field.
    /// Servers should immediately send response headers on the stream before sleeping
    /// for any specified response delay and/or sending the first message so that
    /// clients can be unblocked reading response headers.
    /// If a response definition is not specified OR is specified, but response data
    /// is empty, the server should skip sending anything on the stream. When there
    /// are no responses to send, servers should throw an error if one is provided
    /// and return without error if one is not. Stream headers and trailers should
    /// still be set on the stream if provided regardless of whether a response is
    /// sent or an error is thrown.
    fn server_stream(
        &self,
        ctx: Context,
        request: OwnedView<
            crate::proto::connectrpc::conformance::v1::ServerStreamRequestView<'static>,
        >,
    ) -> impl Future<
        Output = Result<
            (
                Pin<
                    Box<
                        dyn Stream<
                            Item = Result<
                                crate::proto::connectrpc::conformance::v1::ServerStreamResponse,
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
    /// A client-streaming operation. The first request indicates the response
    /// headers and trailers and also indicates either a response message or an
    /// error to send back.
    /// Response message data is specified as bytes. The service should echo back
    /// request properties, including all request messages in the order they were
    /// received, in the ConformancePayload and then include the message data in
    /// the data field.
    /// If the input stream is empty, the server's response will include no data,
    /// only the request properties (headers, timeout).
    /// Servers should only read the response definition from the first message in
    /// the stream and should ignore any definition set in subsequent messages.
    /// Servers should allow the response definition to be unset in the request and
    /// if it is, set no response headers or trailers and return no response data.
    /// The returned payload should only contain the request info.
    fn client_stream(
        &self,
        ctx: Context,
        requests: Pin<
            Box<
                dyn Stream<
                    Item = Result<
                        OwnedView<
                            crate::proto::connectrpc::conformance::v1::ClientStreamRequestView<
                                'static,
                            >,
                        >,
                        ConnectError,
                    >,
                > + Send,
            >,
        >,
    ) -> impl Future<
        Output = Result<
            (crate::proto::connectrpc::conformance::v1::ClientStreamResponse, Context),
            ConnectError,
        >,
    > + Send;
    /// A bidirectional-streaming operation. The first request indicates the response
    /// headers, response messages, trailers, and an optional error to send back.
    /// The response data should be sent in the order indicated, and the server
    /// should wait between sending response messages as indicated.
    /// Response message data is specified as bytes and should be included in the
    /// data field of the ConformancePayload in each response.
    /// Servers should send responses indicated according to the rules of half duplex
    /// vs. full duplex streams. Once all responses are sent, the server should either
    /// return an error if specified or close the stream without error.
    /// Servers should immediately send response headers on the stream before sleeping
    /// for any specified response delay and/or sending the first message so that
    /// clients can be unblocked reading response headers.
    /// If a response definition is not specified OR is specified, but response data
    /// is empty, the server should skip sending anything on the stream. Stream
    /// headers and trailers should always be set on the stream if provided
    /// regardless of whether a response is sent or an error is thrown.
    /// If the full_duplex field is true:
    /// - the handler should read one request and then send back one response, and
    /// then alternate, reading another request and then sending back another response, etc.
    /// - if the server receives a request and has no responses to send, it
    /// should throw the error specified in the request.
    /// - the service should echo back all request properties in the first response
    /// including the last received request. Subsequent responses should only
    /// echo back the last received request.
    /// - if the response_delay_ms duration is specified, the server should wait the given
    /// duration after reading the request before sending the corresponding
    /// response.
    /// If the full_duplex field is false:
    /// - the handler should read all requests until the client is done sending.
    /// Once all requests are read, the server should then send back any responses
    /// specified in the response definition.
    /// - the server should echo back all request properties, including all request
    /// messages in the order they were received, in the first response. Subsequent
    /// responses should only include the message data in the data field.
    /// - if the response_delay_ms duration is specified, the server should wait that
    /// long in between sending each response message.
    fn bidi_stream(
        &self,
        ctx: Context,
        requests: Pin<
            Box<
                dyn Stream<
                    Item = Result<
                        OwnedView<
                            crate::proto::connectrpc::conformance::v1::BidiStreamRequestView<
                                'static,
                            >,
                        >,
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
                                crate::proto::connectrpc::conformance::v1::BidiStreamResponse,
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
    /// A unary endpoint that the server should not implement and should instead
    /// return an unimplemented error when invoked.
    fn unimplemented(
        &self,
        ctx: Context,
        request: OwnedView<
            crate::proto::connectrpc::conformance::v1::UnimplementedRequestView<'static>,
        >,
    ) -> impl Future<
        Output = Result<
            (crate::proto::connectrpc::conformance::v1::UnimplementedResponse, Context),
            ConnectError,
        >,
    > + Send;
    /// A unary endpoint denoted as having no side effects (i.e. idempotent).
    /// Implementations should use an HTTP GET when invoking this endpoint and
    /// leverage query parameters to send data.
    fn idempotent_unary(
        &self,
        ctx: Context,
        request: OwnedView<
            crate::proto::connectrpc::conformance::v1::IdempotentUnaryRequestView<
                'static,
            >,
        >,
    ) -> impl Future<
        Output = Result<
            (
                crate::proto::connectrpc::conformance::v1::IdempotentUnaryResponse,
                Context,
            ),
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
pub trait ConformanceServiceExt: ConformanceService {
    /// Register this service implementation with a Router.
    ///
    /// Takes ownership of the `Arc<Self>` and returns a new Router with
    /// this service's methods registered.
    fn register(self: Arc<Self>, router: Router) -> Router;
}
impl<S: ConformanceService> ConformanceServiceExt for S {
    fn register(self: Arc<Self>, router: Router) -> Router {
        router
            .route_view(
                CONFORMANCE_SERVICE_SERVICE_NAME,
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
                CONFORMANCE_SERVICE_SERVICE_NAME,
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
                CONFORMANCE_SERVICE_SERVICE_NAME,
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
                CONFORMANCE_SERVICE_SERVICE_NAME,
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
                CONFORMANCE_SERVICE_SERVICE_NAME,
                "Unimplemented",
                {
                    let svc = Arc::clone(&self);
                    view_handler_fn(move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.unimplemented(ctx, req).await }
                    })
                },
            )
            .route_view_idempotent(
                CONFORMANCE_SERVICE_SERVICE_NAME,
                "IdempotentUnary",
                {
                    let svc = Arc::clone(&self);
                    view_handler_fn(move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.idempotent_unary(ctx, req).await }
                    })
                },
            )
    }
}
/// Monomorphic dispatcher for `ConformanceService`.
///
/// Unlike `.register(Router)` which type-erases each method into an `Arc<dyn ErasedHandler>` stored in a `HashMap`, this struct dispatches via a compile-time `match` on method name: no vtable, no hash lookup.
///
/// # Example
///
/// ```rust,ignore
/// use connectrpc::ConnectRpcService;
///
/// let server = ConformanceServiceServer::new(MyImpl);
/// let service = ConnectRpcService::new(server);
/// // hand `service` to axum/hyper as a fallback_service
/// ```
pub struct ConformanceServiceServer<T> {
    inner: Arc<T>,
}
impl<T: ConformanceService> ConformanceServiceServer<T> {
    /// Wrap a service implementation in a monomorphic dispatcher.
    pub fn new(service: T) -> Self {
        Self { inner: Arc::new(service) }
    }
    /// Wrap an already-`Arc`'d service implementation.
    pub fn from_arc(inner: Arc<T>) -> Self {
        Self { inner }
    }
}
impl<T> Clone for ConformanceServiceServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
impl<T: ConformanceService> Dispatcher for ConformanceServiceServer<T> {
    #[inline]
    fn lookup(&self, path: &str) -> Option<__crpc_codegen::MethodDescriptor> {
        let method = path.strip_prefix("connectrpc.conformance.v1.ConformanceService/")?;
        match method {
            "Unary" => Some(__crpc_codegen::MethodDescriptor::unary(false)),
            "ServerStream" => Some(__crpc_codegen::MethodDescriptor::server_streaming()),
            "ClientStream" => Some(__crpc_codegen::MethodDescriptor::client_streaming()),
            "BidiStream" => Some(__crpc_codegen::MethodDescriptor::bidi_streaming()),
            "Unimplemented" => Some(__crpc_codegen::MethodDescriptor::unary(false)),
            "IdempotentUnary" => Some(__crpc_codegen::MethodDescriptor::unary(true)),
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
        let Some(method) = path
            .strip_prefix("connectrpc.conformance.v1.ConformanceService/") else {
            return __crpc_codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "Unary" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = __crpc_codegen::decode_request_view::<
                        crate::proto::connectrpc::conformance::v1::UnaryRequestView,
                    >(request, format)?;
                    let (res, ctx) = svc.unary(ctx, req).await?;
                    let bytes = __crpc_codegen::encode_response(&res, format)?;
                    Ok((bytes, ctx))
                })
            }
            "Unimplemented" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = __crpc_codegen::decode_request_view::<
                        crate::proto::connectrpc::conformance::v1::UnimplementedRequestView,
                    >(request, format)?;
                    let (res, ctx) = svc.unimplemented(ctx, req).await?;
                    let bytes = __crpc_codegen::encode_response(&res, format)?;
                    Ok((bytes, ctx))
                })
            }
            "IdempotentUnary" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = __crpc_codegen::decode_request_view::<
                        crate::proto::connectrpc::conformance::v1::IdempotentUnaryRequestView,
                    >(request, format)?;
                    let (res, ctx) = svc.idempotent_unary(ctx, req).await?;
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
        let Some(method) = path
            .strip_prefix("connectrpc.conformance.v1.ConformanceService/") else {
            return __crpc_codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "ServerStream" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = __crpc_codegen::decode_request_view::<
                        crate::proto::connectrpc::conformance::v1::ServerStreamRequestView,
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
        let Some(method) = path
            .strip_prefix("connectrpc.conformance.v1.ConformanceService/") else {
            return __crpc_codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &requests, &format);
        match method {
            "ClientStream" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req_stream = __crpc_codegen::decode_view_request_stream::<
                        crate::proto::connectrpc::conformance::v1::ClientStreamRequestView,
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
        let Some(method) = path
            .strip_prefix("connectrpc.conformance.v1.ConformanceService/") else {
            return __crpc_codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &requests, &format);
        match method {
            "BidiStream" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req_stream = __crpc_codegen::decode_view_request_stream::<
                        crate::proto::connectrpc::conformance::v1::BidiStreamRequestView,
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
/// let client = ConformanceServiceClient::new(conn, config);
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
/// let client = ConformanceServiceClient::new(http, config);
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
pub struct ConformanceServiceClient<T> {
    transport: T,
    config: ClientConfig,
}
impl<T> ConformanceServiceClient<T>
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
    /// Call the Unary RPC. Sends a request to /connectrpc.conformance.v1.ConformanceService/Unary.
    pub async fn unary(
        &self,
        request: crate::proto::connectrpc::conformance::v1::UnaryRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<
                crate::proto::connectrpc::conformance::v1::UnaryResponseView<'static>,
            >,
        >,
        ConnectError,
    > {
        self.unary_with_options(request, CallOptions::default()).await
    }
    /// Call the Unary RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn unary_with_options(
        &self,
        request: crate::proto::connectrpc::conformance::v1::UnaryRequest,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<
                crate::proto::connectrpc::conformance::v1::UnaryResponseView<'static>,
            >,
        >,
        ConnectError,
    > {
        call_unary(
                &self.transport,
                &self.config,
                "connectrpc.conformance.v1.ConformanceService",
                "Unary",
                request,
                options,
            )
            .await
    }
    /// Call the ServerStream RPC. Sends a request to /connectrpc.conformance.v1.ConformanceService/ServerStream.
    pub async fn server_stream(
        &self,
        request: crate::proto::connectrpc::conformance::v1::ServerStreamRequest,
    ) -> Result<
        ::connectrpc::client::ServerStream<
            T::ResponseBody,
            crate::proto::connectrpc::conformance::v1::ServerStreamResponseView<'static>,
        >,
        ConnectError,
    > {
        self.server_stream_with_options(request, CallOptions::default()).await
    }
    /// Call the ServerStream RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn server_stream_with_options(
        &self,
        request: crate::proto::connectrpc::conformance::v1::ServerStreamRequest,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::ServerStream<
            T::ResponseBody,
            crate::proto::connectrpc::conformance::v1::ServerStreamResponseView<'static>,
        >,
        ConnectError,
    > {
        call_server_stream(
                &self.transport,
                &self.config,
                "connectrpc.conformance.v1.ConformanceService",
                "ServerStream",
                request,
                options,
            )
            .await
    }
    /// Call the ClientStream RPC. Sends a request to /connectrpc.conformance.v1.ConformanceService/ClientStream.
    pub async fn client_stream(
        &self,
        requests: impl IntoIterator<
            Item = crate::proto::connectrpc::conformance::v1::ClientStreamRequest,
        >,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<
                crate::proto::connectrpc::conformance::v1::ClientStreamResponseView<
                    'static,
                >,
            >,
        >,
        ConnectError,
    > {
        self.client_stream_with_options(requests, CallOptions::default()).await
    }
    /// Call the ClientStream RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn client_stream_with_options(
        &self,
        requests: impl IntoIterator<
            Item = crate::proto::connectrpc::conformance::v1::ClientStreamRequest,
        >,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<
                crate::proto::connectrpc::conformance::v1::ClientStreamResponseView<
                    'static,
                >,
            >,
        >,
        ConnectError,
    > {
        call_client_stream(
                &self.transport,
                &self.config,
                "connectrpc.conformance.v1.ConformanceService",
                "ClientStream",
                requests,
                options,
            )
            .await
    }
    /// Call the BidiStream RPC. Sends a request to /connectrpc.conformance.v1.ConformanceService/BidiStream.
    pub async fn bidi_stream(
        &self,
    ) -> Result<
        ::connectrpc::client::BidiStream<
            T::ResponseBody,
            crate::proto::connectrpc::conformance::v1::BidiStreamRequest,
            crate::proto::connectrpc::conformance::v1::BidiStreamResponseView<'static>,
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
            crate::proto::connectrpc::conformance::v1::BidiStreamRequest,
            crate::proto::connectrpc::conformance::v1::BidiStreamResponseView<'static>,
        >,
        ConnectError,
    > {
        call_bidi_stream(
                &self.transport,
                &self.config,
                "connectrpc.conformance.v1.ConformanceService",
                "BidiStream",
                options,
            )
            .await
    }
    /// Call the Unimplemented RPC. Sends a request to /connectrpc.conformance.v1.ConformanceService/Unimplemented.
    pub async fn unimplemented(
        &self,
        request: crate::proto::connectrpc::conformance::v1::UnimplementedRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<
                crate::proto::connectrpc::conformance::v1::UnimplementedResponseView<
                    'static,
                >,
            >,
        >,
        ConnectError,
    > {
        self.unimplemented_with_options(request, CallOptions::default()).await
    }
    /// Call the Unimplemented RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn unimplemented_with_options(
        &self,
        request: crate::proto::connectrpc::conformance::v1::UnimplementedRequest,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<
                crate::proto::connectrpc::conformance::v1::UnimplementedResponseView<
                    'static,
                >,
            >,
        >,
        ConnectError,
    > {
        call_unary(
                &self.transport,
                &self.config,
                "connectrpc.conformance.v1.ConformanceService",
                "Unimplemented",
                request,
                options,
            )
            .await
    }
    /// Call the IdempotentUnary RPC. Sends a request to /connectrpc.conformance.v1.ConformanceService/IdempotentUnary.
    pub async fn idempotent_unary(
        &self,
        request: crate::proto::connectrpc::conformance::v1::IdempotentUnaryRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<
                crate::proto::connectrpc::conformance::v1::IdempotentUnaryResponseView<
                    'static,
                >,
            >,
        >,
        ConnectError,
    > {
        self.idempotent_unary_with_options(request, CallOptions::default()).await
    }
    /// Call the IdempotentUnary RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn idempotent_unary_with_options(
        &self,
        request: crate::proto::connectrpc::conformance::v1::IdempotentUnaryRequest,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<
                crate::proto::connectrpc::conformance::v1::IdempotentUnaryResponseView<
                    'static,
                >,
            >,
        >,
        ConnectError,
    > {
        call_unary(
                &self.transport,
                &self.config,
                "connectrpc.conformance.v1.ConformanceService",
                "IdempotentUnary",
                request,
                options,
            )
            .await
    }
}
