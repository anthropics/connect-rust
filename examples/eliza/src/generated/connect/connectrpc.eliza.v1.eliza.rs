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
pub const ELIZA_SERVICE_SERVICE_NAME: &str = "connectrpc.eliza.v1.ElizaService";
/// ElizaService provides a way to talk to Eliza, a port of the DOCTOR script
/// for Joseph Weizenbaum's original ELIZA program. Created in the mid-1960s at
/// the MIT Artificial Intelligence Laboratory, ELIZA demonstrates the
/// superficiality of human-computer communication. DOCTOR simulates a
/// psychotherapist, and is commonly found as an Easter egg in emacs
/// distributions.
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
pub trait ElizaService: Send + Sync + 'static {
    /// Say is a unary RPC. Eliza responds to the prompt with a single sentence.
    fn say(
        &self,
        ctx: Context,
        request: OwnedView<crate::proto::connectrpc::eliza::v1::SayRequestView<'static>>,
    ) -> impl Future<
        Output = Result<
            (crate::proto::connectrpc::eliza::v1::SayResponse, Context),
            ConnectError,
        >,
    > + Send;
    /// Converse is a bidirectional RPC. The caller may exchange multiple
    /// back-and-forth messages with Eliza over a long-lived connection. Eliza
    /// responds to each ConverseRequest with a ConverseResponse.
    fn converse(
        &self,
        ctx: Context,
        requests: Pin<
            Box<
                dyn Stream<
                    Item = Result<
                        OwnedView<
                            crate::proto::connectrpc::eliza::v1::ConverseRequestView<
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
                                crate::proto::connectrpc::eliza::v1::ConverseResponse,
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
    /// Introduce is a server streaming RPC. Given the caller's name, Eliza
    /// returns a stream of sentences to introduce itself.
    fn introduce(
        &self,
        ctx: Context,
        request: OwnedView<
            crate::proto::connectrpc::eliza::v1::IntroduceRequestView<'static>,
        >,
    ) -> impl Future<
        Output = Result<
            (
                Pin<
                    Box<
                        dyn Stream<
                            Item = Result<
                                crate::proto::connectrpc::eliza::v1::IntroduceResponse,
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
pub trait ElizaServiceExt: ElizaService {
    /// Register this service implementation with a Router.
    ///
    /// Takes ownership of the `Arc<Self>` and returns a new Router with
    /// this service's methods registered.
    fn register(self: Arc<Self>, router: Router) -> Router;
}
impl<S: ElizaService> ElizaServiceExt for S {
    fn register(self: Arc<Self>, router: Router) -> Router {
        router
            .route_view_idempotent(
                ELIZA_SERVICE_SERVICE_NAME,
                "Say",
                {
                    let svc = Arc::clone(&self);
                    view_handler_fn(move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.say(ctx, req).await }
                    })
                },
            )
            .route_view_bidi_stream(
                ELIZA_SERVICE_SERVICE_NAME,
                "Converse",
                view_bidi_streaming_handler_fn({
                    let svc = Arc::clone(&self);
                    move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.converse(ctx, req).await }
                    }
                }),
            )
            .route_view_server_stream(
                ELIZA_SERVICE_SERVICE_NAME,
                "Introduce",
                view_streaming_handler_fn({
                    let svc = Arc::clone(&self);
                    move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.introduce(ctx, req).await }
                    }
                }),
            )
    }
}
/// Monomorphic dispatcher for `ElizaService`.
///
/// Unlike `.register(Router)` which type-erases each method into an `Arc<dyn ErasedHandler>` stored in a `HashMap`, this struct dispatches via a compile-time `match` on method name: no vtable, no hash lookup.
///
/// # Example
///
/// ```rust,ignore
/// use connectrpc::ConnectRpcService;
///
/// let server = ElizaServiceServer::new(MyImpl);
/// let service = ConnectRpcService::new(server);
/// // hand `service` to axum/hyper as a fallback_service
/// ```
pub struct ElizaServiceServer<T> {
    inner: Arc<T>,
}
impl<T: ElizaService> ElizaServiceServer<T> {
    /// Wrap a service implementation in a monomorphic dispatcher.
    pub fn new(service: T) -> Self {
        Self { inner: Arc::new(service) }
    }
    /// Wrap an already-`Arc`'d service implementation.
    pub fn from_arc(inner: Arc<T>) -> Self {
        Self { inner }
    }
}
impl<T> Clone for ElizaServiceServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
impl<T: ElizaService> Dispatcher for ElizaServiceServer<T> {
    #[inline]
    fn lookup(&self, path: &str) -> Option<__crpc_codegen::MethodDescriptor> {
        let method = path.strip_prefix("connectrpc.eliza.v1.ElizaService/")?;
        match method {
            "Say" => Some(__crpc_codegen::MethodDescriptor::unary(true)),
            "Converse" => Some(__crpc_codegen::MethodDescriptor::bidi_streaming()),
            "Introduce" => Some(__crpc_codegen::MethodDescriptor::server_streaming()),
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
        let Some(method) = path.strip_prefix("connectrpc.eliza.v1.ElizaService/") else {
            return __crpc_codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "Say" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = __crpc_codegen::decode_request_view::<
                        crate::proto::connectrpc::eliza::v1::SayRequestView,
                    >(request, format)?;
                    let (res, ctx) = svc.say(ctx, req).await?;
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
        let Some(method) = path.strip_prefix("connectrpc.eliza.v1.ElizaService/") else {
            return __crpc_codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "Introduce" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = __crpc_codegen::decode_request_view::<
                        crate::proto::connectrpc::eliza::v1::IntroduceRequestView,
                    >(request, format)?;
                    let (resp_stream, ctx) = svc.introduce(ctx, req).await?;
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
        let Some(method) = path.strip_prefix("connectrpc.eliza.v1.ElizaService/") else {
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
        let Some(method) = path.strip_prefix("connectrpc.eliza.v1.ElizaService/") else {
            return __crpc_codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &requests, &format);
        match method {
            "Converse" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req_stream = __crpc_codegen::decode_view_request_stream::<
                        crate::proto::connectrpc::eliza::v1::ConverseRequestView,
                    >(requests, format);
                    let (resp_stream, ctx) = svc.converse(ctx, req_stream).await?;
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
/// let client = ElizaServiceClient::new(conn, config);
/// let response = client.say(request).await?;
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
/// let client = ElizaServiceClient::new(http, config);
/// let response = client.say(request).await?;
/// ```
///
/// # Working with the response
///
/// Unary calls return [`UnaryResponse<OwnedView<FooView>>`](::connectrpc::client::UnaryResponse).
/// The `OwnedView` derefs to the view, so field access is zero-copy:
///
/// ```rust,ignore
/// let resp = client.say(request).await?.into_view();
/// let name: &str = resp.name;  // borrow into the response buffer
/// ```
///
/// If you need the owned struct (e.g. to store or pass by value), use
/// [`into_owned()`](::connectrpc::client::UnaryResponse::into_owned):
///
/// ```rust,ignore
/// let owned = client.say(request).await?.into_owned();
/// ```
#[derive(Clone)]
pub struct ElizaServiceClient<T> {
    transport: T,
    config: ClientConfig,
}
impl<T> ElizaServiceClient<T>
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
    /// Call the Say RPC. Sends a request to /connectrpc.eliza.v1.ElizaService/Say.
    pub async fn say(
        &self,
        request: crate::proto::connectrpc::eliza::v1::SayRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<crate::proto::connectrpc::eliza::v1::SayResponseView<'static>>,
        >,
        ConnectError,
    > {
        self.say_with_options(request, CallOptions::default()).await
    }
    /// Call the Say RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn say_with_options(
        &self,
        request: crate::proto::connectrpc::eliza::v1::SayRequest,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<crate::proto::connectrpc::eliza::v1::SayResponseView<'static>>,
        >,
        ConnectError,
    > {
        call_unary(
                &self.transport,
                &self.config,
                "connectrpc.eliza.v1.ElizaService",
                "Say",
                request,
                options,
            )
            .await
    }
    /// Call the Converse RPC. Sends a request to /connectrpc.eliza.v1.ElizaService/Converse.
    pub async fn converse(
        &self,
    ) -> Result<
        ::connectrpc::client::BidiStream<
            T::ResponseBody,
            crate::proto::connectrpc::eliza::v1::ConverseRequest,
            crate::proto::connectrpc::eliza::v1::ConverseResponseView<'static>,
        >,
        ConnectError,
    > {
        self.converse_with_options(CallOptions::default()).await
    }
    /// Call the Converse RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn converse_with_options(
        &self,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::BidiStream<
            T::ResponseBody,
            crate::proto::connectrpc::eliza::v1::ConverseRequest,
            crate::proto::connectrpc::eliza::v1::ConverseResponseView<'static>,
        >,
        ConnectError,
    > {
        call_bidi_stream(
                &self.transport,
                &self.config,
                "connectrpc.eliza.v1.ElizaService",
                "Converse",
                options,
            )
            .await
    }
    /// Call the Introduce RPC. Sends a request to /connectrpc.eliza.v1.ElizaService/Introduce.
    pub async fn introduce(
        &self,
        request: crate::proto::connectrpc::eliza::v1::IntroduceRequest,
    ) -> Result<
        ::connectrpc::client::ServerStream<
            T::ResponseBody,
            crate::proto::connectrpc::eliza::v1::IntroduceResponseView<'static>,
        >,
        ConnectError,
    > {
        self.introduce_with_options(request, CallOptions::default()).await
    }
    /// Call the Introduce RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn introduce_with_options(
        &self,
        request: crate::proto::connectrpc::eliza::v1::IntroduceRequest,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::ServerStream<
            T::ResponseBody,
            crate::proto::connectrpc::eliza::v1::IntroduceResponseView<'static>,
        >,
        ConnectError,
    > {
        call_server_stream(
                &self.transport,
                &self.config,
                "connectrpc.eliza.v1.ElizaService",
                "Introduce",
                request,
                options,
            )
            .await
    }
}
