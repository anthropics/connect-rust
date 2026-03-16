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
pub const MATH_SERVICE_SERVICE_NAME: &str = "anthropic.connectrpc.math.v1.MathService";
/// MathService provides basic math operations.
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
pub trait MathService: Send + Sync + 'static {
    /// Add returns the sum of two numbers.
    fn add(
        &self,
        ctx: Context,
        request: OwnedView<
            crate::proto::anthropic::connectrpc::math::v1::AddRequestView<'static>,
        >,
    ) -> impl Future<
        Output = Result<
            (crate::proto::anthropic::connectrpc::math::v1::AddResponse, Context),
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
pub trait MathServiceExt: MathService {
    /// Register this service implementation with a Router.
    ///
    /// Takes ownership of the `Arc<Self>` and returns a new Router with
    /// this service's methods registered.
    fn register(self: Arc<Self>, router: Router) -> Router;
}
impl<S: MathService> MathServiceExt for S {
    fn register(self: Arc<Self>, router: Router) -> Router {
        router
            .route_view(
                MATH_SERVICE_SERVICE_NAME,
                "Add",
                {
                    let svc = Arc::clone(&self);
                    view_handler_fn(move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.add(ctx, req).await }
                    })
                },
            )
    }
}
/// Monomorphic dispatcher for `MathService`.
///
/// Unlike `.register(Router)` which type-erases each method into an `Arc<dyn ErasedHandler>` stored in a `HashMap`, this struct dispatches via a compile-time `match` on method name: no vtable, no hash lookup.
///
/// # Example
///
/// ```rust,ignore
/// use connectrpc::ConnectRpcService;
///
/// let server = MathServiceServer::new(MyImpl);
/// let service = ConnectRpcService::new(server);
/// // hand `service` to axum/hyper as a fallback_service
/// ```
pub struct MathServiceServer<T> {
    inner: Arc<T>,
}
impl<T: MathService> MathServiceServer<T> {
    /// Wrap a service implementation in a monomorphic dispatcher.
    pub fn new(service: T) -> Self {
        Self { inner: Arc::new(service) }
    }
    /// Wrap an already-`Arc`'d service implementation.
    pub fn from_arc(inner: Arc<T>) -> Self {
        Self { inner }
    }
}
impl<T> Clone for MathServiceServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
impl<T: MathService> Dispatcher for MathServiceServer<T> {
    #[inline]
    fn lookup(&self, path: &str) -> Option<__crpc_codegen::MethodDescriptor> {
        let method = path.strip_prefix("anthropic.connectrpc.math.v1.MathService/")?;
        match method {
            "Add" => Some(__crpc_codegen::MethodDescriptor::unary(false)),
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
        let Some(method) = path.strip_prefix("anthropic.connectrpc.math.v1.MathService/")
        else {
            return __crpc_codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "Add" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = __crpc_codegen::decode_request_view::<
                        crate::proto::anthropic::connectrpc::math::v1::AddRequestView,
                    >(request, format)?;
                    let (res, ctx) = svc.add(ctx, req).await?;
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
        let Some(method) = path.strip_prefix("anthropic.connectrpc.math.v1.MathService/")
        else {
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
        let Some(method) = path.strip_prefix("anthropic.connectrpc.math.v1.MathService/")
        else {
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
        let Some(method) = path.strip_prefix("anthropic.connectrpc.math.v1.MathService/")
        else {
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
/// let client = MathServiceClient::new(conn, config);
/// let response = client.add(request).await?;
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
/// let client = MathServiceClient::new(http, config);
/// let response = client.add(request).await?;
/// ```
///
/// # Working with the response
///
/// Unary calls return [`UnaryResponse<OwnedView<FooView>>`](::connectrpc::client::UnaryResponse).
/// The `OwnedView` derefs to the view, so field access is zero-copy:
///
/// ```rust,ignore
/// let resp = client.add(request).await?.into_view();
/// let name: &str = resp.name;  // borrow into the response buffer
/// ```
///
/// If you need the owned struct (e.g. to store or pass by value), use
/// [`into_owned()`](::connectrpc::client::UnaryResponse::into_owned):
///
/// ```rust,ignore
/// let owned = client.add(request).await?.into_owned();
/// ```
#[derive(Clone)]
pub struct MathServiceClient<T> {
    transport: T,
    config: ClientConfig,
}
impl<T> MathServiceClient<T>
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
    /// Call the Add RPC. Sends a request to /anthropic.connectrpc.math.v1.MathService/Add.
    pub async fn add(
        &self,
        request: crate::proto::anthropic::connectrpc::math::v1::AddRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<
                crate::proto::anthropic::connectrpc::math::v1::AddResponseView<'static>,
            >,
        >,
        ConnectError,
    > {
        self.add_with_options(request, CallOptions::default()).await
    }
    /// Call the Add RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn add_with_options(
        &self,
        request: crate::proto::anthropic::connectrpc::math::v1::AddRequest,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<
                crate::proto::anthropic::connectrpc::math::v1::AddResponseView<'static>,
            >,
        >,
        ConnectError,
    > {
        call_unary(
                &self.transport,
                &self.config,
                "anthropic.connectrpc.math.v1.MathService",
                "Add",
                request,
                options,
            )
            .await
    }
}
