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
pub const WELL_KNOWN_TYPES_SERVICE_SERVICE_NAME: &str = "anthropic.connectrpc.wkt.v1.WellKnownTypesService";
/// WellKnownTypesService provides operations using Timestamp, Duration, and Struct.
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
pub trait WellKnownTypesService: Send + Sync + 'static {
    /// CreateEvent creates an event with a timestamp.
    fn create_event(
        &self,
        ctx: Context,
        request: OwnedView<
            crate::proto::anthropic::connectrpc::wkt::v1::CreateEventRequestView<'static>,
        >,
    ) -> impl Future<
        Output = Result<
            (crate::proto::anthropic::connectrpc::wkt::v1::CreateEventResponse, Context),
            ConnectError,
        >,
    > + Send;
    /// CalculateDuration calculates the duration between two timestamps.
    fn calculate_duration(
        &self,
        ctx: Context,
        request: OwnedView<
            crate::proto::anthropic::connectrpc::wkt::v1::CalculateDurationRequestView<
                'static,
            >,
        >,
    ) -> impl Future<
        Output = Result<
            (
                crate::proto::anthropic::connectrpc::wkt::v1::CalculateDurationResponse,
                Context,
            ),
            ConnectError,
        >,
    > + Send;
    /// ProcessMetadata processes arbitrary metadata as a Struct.
    fn process_metadata(
        &self,
        ctx: Context,
        request: OwnedView<
            crate::proto::anthropic::connectrpc::wkt::v1::ProcessMetadataRequestView<
                'static,
            >,
        >,
    ) -> impl Future<
        Output = Result<
            (
                crate::proto::anthropic::connectrpc::wkt::v1::ProcessMetadataResponse,
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
pub trait WellKnownTypesServiceExt: WellKnownTypesService {
    /// Register this service implementation with a Router.
    ///
    /// Takes ownership of the `Arc<Self>` and returns a new Router with
    /// this service's methods registered.
    fn register(self: Arc<Self>, router: Router) -> Router;
}
impl<S: WellKnownTypesService> WellKnownTypesServiceExt for S {
    fn register(self: Arc<Self>, router: Router) -> Router {
        router
            .route_view(
                WELL_KNOWN_TYPES_SERVICE_SERVICE_NAME,
                "CreateEvent",
                {
                    let svc = Arc::clone(&self);
                    view_handler_fn(move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.create_event(ctx, req).await }
                    })
                },
            )
            .route_view(
                WELL_KNOWN_TYPES_SERVICE_SERVICE_NAME,
                "CalculateDuration",
                {
                    let svc = Arc::clone(&self);
                    view_handler_fn(move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.calculate_duration(ctx, req).await }
                    })
                },
            )
            .route_view(
                WELL_KNOWN_TYPES_SERVICE_SERVICE_NAME,
                "ProcessMetadata",
                {
                    let svc = Arc::clone(&self);
                    view_handler_fn(move |ctx, req| {
                        let svc = Arc::clone(&svc);
                        async move { svc.process_metadata(ctx, req).await }
                    })
                },
            )
    }
}
/// Monomorphic dispatcher for `WellKnownTypesService`.
///
/// Unlike `.register(Router)` which type-erases each method into an `Arc<dyn ErasedHandler>` stored in a `HashMap`, this struct dispatches via a compile-time `match` on method name: no vtable, no hash lookup.
///
/// # Example
///
/// ```rust,ignore
/// use connectrpc::ConnectRpcService;
///
/// let server = WellKnownTypesServiceServer::new(MyImpl);
/// let service = ConnectRpcService::new(server);
/// // hand `service` to axum/hyper as a fallback_service
/// ```
pub struct WellKnownTypesServiceServer<T> {
    inner: Arc<T>,
}
impl<T: WellKnownTypesService> WellKnownTypesServiceServer<T> {
    /// Wrap a service implementation in a monomorphic dispatcher.
    pub fn new(service: T) -> Self {
        Self { inner: Arc::new(service) }
    }
    /// Wrap an already-`Arc`'d service implementation.
    pub fn from_arc(inner: Arc<T>) -> Self {
        Self { inner }
    }
}
impl<T> Clone for WellKnownTypesServiceServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
impl<T: WellKnownTypesService> Dispatcher for WellKnownTypesServiceServer<T> {
    #[inline]
    fn lookup(&self, path: &str) -> Option<__crpc_codegen::MethodDescriptor> {
        let method = path
            .strip_prefix("anthropic.connectrpc.wkt.v1.WellKnownTypesService/")?;
        match method {
            "CreateEvent" => Some(__crpc_codegen::MethodDescriptor::unary(false)),
            "CalculateDuration" => Some(__crpc_codegen::MethodDescriptor::unary(false)),
            "ProcessMetadata" => Some(__crpc_codegen::MethodDescriptor::unary(false)),
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
            .strip_prefix("anthropic.connectrpc.wkt.v1.WellKnownTypesService/") else {
            return __crpc_codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "CreateEvent" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = __crpc_codegen::decode_request_view::<
                        crate::proto::anthropic::connectrpc::wkt::v1::CreateEventRequestView,
                    >(request, format)?;
                    let (res, ctx) = svc.create_event(ctx, req).await?;
                    let bytes = __crpc_codegen::encode_response(&res, format)?;
                    Ok((bytes, ctx))
                })
            }
            "CalculateDuration" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = __crpc_codegen::decode_request_view::<
                        crate::proto::anthropic::connectrpc::wkt::v1::CalculateDurationRequestView,
                    >(request, format)?;
                    let (res, ctx) = svc.calculate_duration(ctx, req).await?;
                    let bytes = __crpc_codegen::encode_response(&res, format)?;
                    Ok((bytes, ctx))
                })
            }
            "ProcessMetadata" => {
                let svc = Arc::clone(&self.inner);
                Box::pin(async move {
                    let req = __crpc_codegen::decode_request_view::<
                        crate::proto::anthropic::connectrpc::wkt::v1::ProcessMetadataRequestView,
                    >(request, format)?;
                    let (res, ctx) = svc.process_metadata(ctx, req).await?;
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
            .strip_prefix("anthropic.connectrpc.wkt.v1.WellKnownTypesService/") else {
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
        let Some(method) = path
            .strip_prefix("anthropic.connectrpc.wkt.v1.WellKnownTypesService/") else {
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
        let Some(method) = path
            .strip_prefix("anthropic.connectrpc.wkt.v1.WellKnownTypesService/") else {
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
/// let client = WellKnownTypesServiceClient::new(conn, config);
/// let response = client.create_event(request).await?;
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
/// let client = WellKnownTypesServiceClient::new(http, config);
/// let response = client.create_event(request).await?;
/// ```
///
/// # Working with the response
///
/// Unary calls return [`UnaryResponse<OwnedView<FooView>>`](::connectrpc::client::UnaryResponse).
/// The `OwnedView` derefs to the view, so field access is zero-copy:
///
/// ```rust,ignore
/// let resp = client.create_event(request).await?.into_view();
/// let name: &str = resp.name;  // borrow into the response buffer
/// ```
///
/// If you need the owned struct (e.g. to store or pass by value), use
/// [`into_owned()`](::connectrpc::client::UnaryResponse::into_owned):
///
/// ```rust,ignore
/// let owned = client.create_event(request).await?.into_owned();
/// ```
#[derive(Clone)]
pub struct WellKnownTypesServiceClient<T> {
    transport: T,
    config: ClientConfig,
}
impl<T> WellKnownTypesServiceClient<T>
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
    /// Call the CreateEvent RPC. Sends a request to /anthropic.connectrpc.wkt.v1.WellKnownTypesService/CreateEvent.
    pub async fn create_event(
        &self,
        request: crate::proto::anthropic::connectrpc::wkt::v1::CreateEventRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<
                crate::proto::anthropic::connectrpc::wkt::v1::CreateEventResponseView<
                    'static,
                >,
            >,
        >,
        ConnectError,
    > {
        self.create_event_with_options(request, CallOptions::default()).await
    }
    /// Call the CreateEvent RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn create_event_with_options(
        &self,
        request: crate::proto::anthropic::connectrpc::wkt::v1::CreateEventRequest,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<
                crate::proto::anthropic::connectrpc::wkt::v1::CreateEventResponseView<
                    'static,
                >,
            >,
        >,
        ConnectError,
    > {
        call_unary(
                &self.transport,
                &self.config,
                "anthropic.connectrpc.wkt.v1.WellKnownTypesService",
                "CreateEvent",
                request,
                options,
            )
            .await
    }
    /// Call the CalculateDuration RPC. Sends a request to /anthropic.connectrpc.wkt.v1.WellKnownTypesService/CalculateDuration.
    pub async fn calculate_duration(
        &self,
        request: crate::proto::anthropic::connectrpc::wkt::v1::CalculateDurationRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<
                crate::proto::anthropic::connectrpc::wkt::v1::CalculateDurationResponseView<
                    'static,
                >,
            >,
        >,
        ConnectError,
    > {
        self.calculate_duration_with_options(request, CallOptions::default()).await
    }
    /// Call the CalculateDuration RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn calculate_duration_with_options(
        &self,
        request: crate::proto::anthropic::connectrpc::wkt::v1::CalculateDurationRequest,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<
                crate::proto::anthropic::connectrpc::wkt::v1::CalculateDurationResponseView<
                    'static,
                >,
            >,
        >,
        ConnectError,
    > {
        call_unary(
                &self.transport,
                &self.config,
                "anthropic.connectrpc.wkt.v1.WellKnownTypesService",
                "CalculateDuration",
                request,
                options,
            )
            .await
    }
    /// Call the ProcessMetadata RPC. Sends a request to /anthropic.connectrpc.wkt.v1.WellKnownTypesService/ProcessMetadata.
    pub async fn process_metadata(
        &self,
        request: crate::proto::anthropic::connectrpc::wkt::v1::ProcessMetadataRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<
                crate::proto::anthropic::connectrpc::wkt::v1::ProcessMetadataResponseView<
                    'static,
                >,
            >,
        >,
        ConnectError,
    > {
        self.process_metadata_with_options(request, CallOptions::default()).await
    }
    /// Call the ProcessMetadata RPC with explicit per-call options. Options override [`ClientConfig`] defaults.
    pub async fn process_metadata_with_options(
        &self,
        request: crate::proto::anthropic::connectrpc::wkt::v1::ProcessMetadataRequest,
        options: CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            OwnedView<
                crate::proto::anthropic::connectrpc::wkt::v1::ProcessMetadataResponseView<
                    'static,
                >,
            >,
        >,
        ConnectError,
    > {
        call_unary(
                &self.transport,
                &self.config,
                "anthropic.connectrpc.wkt.v1.WellKnownTypesService",
                "ProcessMetadata",
                request,
                options,
            )
            .await
    }
}
