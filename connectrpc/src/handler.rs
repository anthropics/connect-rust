//! Handler traits for implementing RPC methods.
//!
//! This module defines the traits that RPC method implementations must satisfy,
//! supporting both unary and streaming RPC patterns.

use std::pin::Pin;
use std::sync::Arc;

use buffa::Message;
use buffa::view::MessageView;
use buffa::view::OwnedView;
use bytes::Bytes;
use futures::Stream;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::codec::CodecFormat;
use crate::error::ConnectError;

/// Decode a request message from bytes using the specified codec format.
///
/// This helper is used by both unary and streaming handler wrappers to avoid duplication.
pub(crate) fn decode_request<Req>(request: &Bytes, format: CodecFormat) -> Result<Req, ConnectError>
where
    Req: Message + DeserializeOwned,
{
    match format {
        CodecFormat::Proto => Req::decode_from_slice(&request[..]).map_err(|e| {
            ConnectError::invalid_argument(format!("failed to decode proto request: {e}"))
        }),
        CodecFormat::Json => serde_json::from_slice(request).map_err(|e| {
            ConnectError::invalid_argument(format!("failed to decode JSON request: {e}"))
        }),
    }
}

/// Encode a response message to bytes using the specified codec format.
///
/// This helper is used by both unary and streaming handler wrappers to avoid duplication.
#[doc(hidden)] // exposed only for dispatcher::codegen (generated code)
pub fn encode_response<Res>(res: &Res, format: CodecFormat) -> Result<Bytes, ConnectError>
where
    Res: Message + Serialize,
{
    match format {
        CodecFormat::Proto => Ok(res.encode_to_bytes()),
        CodecFormat::Json => serde_json::to_vec(res)
            .map(Bytes::from)
            .map_err(|e| ConnectError::internal(format!("failed to encode JSON response: {e}"))),
    }
}

/// Context passed to RPC handlers.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Request headers.
    pub headers: http::HeaderMap,
    /// Response headers to be set by the handler.
    pub response_headers: http::HeaderMap,
    /// Response trailers to be set by the handler.
    pub trailers: http::HeaderMap,
    /// Request timeout/deadline, if specified.
    pub deadline: Option<std::time::Instant>,
    /// Whether to compress the response. `None` uses the server's compression
    /// policy. Set to `Some(false)` to disable compression for this response,
    /// or `Some(true)` to force it.
    pub compress_response: Option<bool>,
}

impl Context {
    /// Create a new context with the given headers.
    pub fn new(headers: http::HeaderMap) -> Self {
        Self {
            headers,
            response_headers: http::HeaderMap::new(),
            trailers: http::HeaderMap::new(),
            deadline: None,
            compress_response: None,
        }
    }

    /// Set the request deadline (absolute `Instant`).
    ///
    /// Used by the server dispatch paths to expose the parsed timeout
    /// to handlers, allowing deadline propagation to downstream calls.
    #[must_use]
    pub fn with_deadline(mut self, deadline: Option<std::time::Instant>) -> Self {
        self.deadline = deadline;
        self
    }

    /// Set a response trailer.
    pub fn set_trailer(&mut self, key: http::header::HeaderName, value: http::header::HeaderValue) {
        self.trailers.insert(key, value);
    }

    /// Set whether to compress the response for this RPC.
    pub fn set_compression(&mut self, enabled: bool) {
        self.compress_response = Some(enabled);
    }

    /// Get a request header value.
    pub fn header(&self, key: &http::header::HeaderName) -> Option<&http::header::HeaderValue> {
        self.headers.get(key)
    }
}

/// Type alias for a boxed future used in handlers.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Trait for unary RPC handlers.
///
/// A unary handler takes a single request message and returns a single response.
///
/// # Context Ownership
///
/// The handler takes ownership of [`Context`] and returns it along with the response.
/// This design is intentional for async compatibility:
///
/// - The returned future is `'static`, meaning it cannot borrow from external state
/// - Taking ownership allows the future to move the `Context` into itself
/// - Using `&mut Context` would require non-`'static` lifetimes, complicating tower integration
///
/// Handlers should set response headers/trailers on the context before returning:
///
/// ```ignore
/// async fn my_handler(mut ctx: Context, req: MyRequest) -> Result<(MyResponse, Context), ConnectError> {
///     ctx.response_headers.insert("x-custom-header", "value".parse().unwrap());
///     Ok((MyResponse::default(), ctx))
/// }
/// ```
pub trait Handler<Req, Res>: Send + Sync + 'static
where
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    /// Handle a unary RPC request.
    fn call(
        &self,
        ctx: Context,
        request: Req,
    ) -> BoxFuture<'static, Result<(Res, Context), ConnectError>>;
}

/// Wrapper that implements Handler for async functions.
pub struct FnHandler<F> {
    f: Arc<F>,
}

impl<F> FnHandler<F> {
    /// Create a new function handler.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F, Fut, Req, Res> Handler<Req, Res> for FnHandler<F>
where
    F: Fn(Context, Req) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(Res, Context), ConnectError>> + Send + 'static,
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    fn call(
        &self,
        ctx: Context,
        request: Req,
    ) -> BoxFuture<'static, Result<(Res, Context), ConnectError>> {
        let f = Arc::clone(&self.f);
        Box::pin(async move { f(ctx, request).await })
    }
}

/// Trait for server streaming RPC handlers.
///
/// A streaming handler takes a single request and returns a stream of responses.
pub trait StreamingHandler<Req, Res>: Send + Sync + 'static
where
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    /// The stream type returned by this handler.
    type Stream: Stream<Item = Result<Res, ConnectError>> + Send + 'static;

    /// Handle a server streaming RPC request.
    fn call(
        &self,
        ctx: Context,
        request: Req,
    ) -> BoxFuture<'static, Result<(Self::Stream, Context), ConnectError>>;
}

/// Wrapper that implements StreamingHandler for async functions.
pub struct FnStreamingHandler<F> {
    f: Arc<F>,
}

impl<F> FnStreamingHandler<F> {
    /// Create a new function streaming handler.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F, Fut, S, Req, Res> StreamingHandler<Req, Res> for FnStreamingHandler<F>
where
    F: Fn(Context, Req) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(S, Context), ConnectError>> + Send + 'static,
    S: Stream<Item = Result<Res, ConnectError>> + Send + 'static,
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    type Stream = S;

    fn call(
        &self,
        ctx: Context,
        request: Req,
    ) -> BoxFuture<'static, Result<(Self::Stream, Context), ConnectError>> {
        let f = Arc::clone(&self.f);
        Box::pin(async move { f(ctx, request).await })
    }
}

/// Helper function to create a streaming handler from an async function.
///
/// This is the recommended way to create streaming handlers from async functions.
///
/// # Example
///
/// ```rust,ignore
/// use connectrpc::{streaming_handler_fn, Context, ConnectError};
/// use futures::stream;
///
/// async fn my_handler(ctx: Context, req: MyRequest) -> Result<(impl Stream<Item = Result<MyResponse, ConnectError>>, Context), ConnectError> {
///     let responses = stream::iter(vec![
///         Ok(MyResponse { ... }),
///         Ok(MyResponse { ... }),
///     ]);
///     Ok((responses, ctx))
/// }
///
/// let router = Router::new()
///     .route_server_stream("my.Service", "Method", streaming_handler_fn(my_handler));
/// ```
pub fn streaming_handler_fn<F, Fut, S, Req, Res>(f: F) -> FnStreamingHandler<F>
where
    F: Fn(Context, Req) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(S, Context), ConnectError>> + Send + 'static,
    S: Stream<Item = Result<Res, ConnectError>> + Send + 'static,
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    FnStreamingHandler::new(f)
}

/// Type-erased handler for use in the router.
pub(crate) trait ErasedHandler: Send + Sync {
    /// Handle a request with raw bytes and specified codec format.
    fn call_erased(
        &self,
        ctx: Context,
        request: Bytes,
        format: CodecFormat,
    ) -> BoxFuture<'static, Result<(Bytes, Context), ConnectError>>;

    /// Check if this is a streaming handler.
    #[allow(dead_code)]
    fn is_streaming(&self) -> bool;
}

/// Wrapper to erase the types from a unary handler.
///
/// The request and response types must implement both buffa::Message (for proto encoding)
/// and serde traits (for JSON encoding).
pub(crate) struct UnaryHandlerWrapper<H, Req, Res>
where
    H: Handler<Req, Res>,
    Req: Message + DeserializeOwned + Send + 'static,
    Res: Message + Serialize + Send + 'static,
{
    handler: Arc<H>,
    _phantom: std::marker::PhantomData<fn(Req) -> Res>,
}

impl<H, Req, Res> UnaryHandlerWrapper<H, Req, Res>
where
    H: Handler<Req, Res>,
    Req: Message + DeserializeOwned + Send + 'static,
    Res: Message + Serialize + Send + 'static,
{
    /// Create a new wrapper around the given handler.
    pub fn new(handler: H) -> Self {
        Self {
            handler: Arc::new(handler),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<H, Req, Res> ErasedHandler for UnaryHandlerWrapper<H, Req, Res>
where
    H: Handler<Req, Res>,
    Req: Message + DeserializeOwned + Send + 'static,
    Res: Message + Serialize + Send + 'static,
{
    fn call_erased(
        &self,
        ctx: Context,
        request: Bytes,
        format: CodecFormat,
    ) -> BoxFuture<'static, Result<(Bytes, Context), ConnectError>> {
        let handler = Arc::clone(&self.handler);
        Box::pin(async move {
            let req: Req = decode_request(&request, format)?;
            let (res, ctx) = handler.call(ctx, req).await?;
            let response_bytes = encode_response(&res, format)?;
            Ok((response_bytes, ctx))
        })
    }

    fn is_streaming(&self) -> bool {
        false
    }
}

/// Type alias for a boxed stream of encoded response bytes.
pub type BoxStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>;

/// Type-erased streaming handler for use in the router.
pub(crate) trait ErasedStreamingHandler: Send + Sync {
    /// Handle a streaming request with raw bytes and specified codec format.
    ///
    /// Returns the initial context (with response headers) and a stream of encoded response bytes.
    /// The stream yields `Result<Bytes, ConnectError>` for each response message.
    fn call_erased(
        &self,
        ctx: Context,
        request: Bytes,
        format: CodecFormat,
    ) -> StreamingHandlerResult;
}

/// Result type for erased streaming handlers.
pub(crate) type StreamingHandlerResult =
    BoxFuture<'static, Result<(BoxStream<Result<Bytes, ConnectError>>, Context), ConnectError>>;

/// Wrapper to erase the types from a server streaming handler.
pub(crate) struct ServerStreamingHandlerWrapper<H, Req, Res>
where
    H: StreamingHandler<Req, Res>,
    Req: Message + DeserializeOwned + Send + 'static,
    Res: Message + Serialize + Send + 'static,
{
    handler: Arc<H>,
    _phantom: std::marker::PhantomData<fn(Req) -> Res>,
}

impl<H, Req, Res> ServerStreamingHandlerWrapper<H, Req, Res>
where
    H: StreamingHandler<Req, Res>,
    Req: Message + DeserializeOwned + Send + 'static,
    Res: Message + Serialize + Send + 'static,
{
    /// Create a new wrapper around the given streaming handler.
    pub fn new(handler: H) -> Self {
        Self {
            handler: Arc::new(handler),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<H, Req, Res> ErasedStreamingHandler for ServerStreamingHandlerWrapper<H, Req, Res>
where
    H: StreamingHandler<Req, Res>,
    Req: Message + DeserializeOwned + Send + 'static,
    Res: Message + Serialize + Send + 'static,
{
    fn call_erased(
        &self,
        ctx: Context,
        request: Bytes,
        format: CodecFormat,
    ) -> StreamingHandlerResult {
        let handler = Arc::clone(&self.handler);
        Box::pin(async move {
            let req: Req = decode_request(&request, format)?;
            let (stream, ctx) = handler.call(ctx, req).await?;

            // Map the stream to encode each response
            // Use .fuse() to make the stream safe to poll after returning None
            let encoded_stream: BoxStream<Result<Bytes, ConnectError>> = {
                use futures::StreamExt as _;
                Box::pin(
                    futures::stream::unfold(
                        (
                            Box::pin(stream)
                                as Pin<Box<dyn Stream<Item = Result<Res, ConnectError>> + Send>>,
                            format,
                        ),
                        async |(mut stream, format)| match stream.next().await {
                            Some(Ok(res)) => {
                                let encoded = encode_response(&res, format);
                                Some((encoded, (stream, format)))
                            }
                            Some(Err(e)) => Some((Err(e), (stream, format))),
                            None => None,
                        },
                    )
                    .fuse(),
                )
            };

            Ok((encoded_stream, ctx))
        })
    }
}

/// Helper function to create a handler from an async function.
///
/// This is the recommended way to create handlers from async functions.
///
/// # Example
///
/// ```rust,ignore
/// use connectrpc::{handler_fn, Context, ConnectError};
///
/// async fn my_handler(ctx: Context, req: MyRequest) -> Result<(MyResponse, Context), ConnectError> {
///     Ok((MyResponse { ... }, ctx))
/// }
///
/// let router = Router::new()
///     .route("my.Service", "Method", handler_fn(my_handler));
/// ```
pub fn handler_fn<F, Fut, Req, Res>(f: F) -> FnHandler<F>
where
    F: Fn(Context, Req) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(Res, Context), ConnectError>> + Send + 'static,
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    FnHandler::new(f)
}

/// Trait for client streaming RPC handlers.
///
/// A client streaming handler receives a stream of request messages and returns a single response.
pub trait ClientStreamingHandler<Req, Res>: Send + Sync + 'static
where
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    /// Handle a client streaming RPC request.
    fn call(
        &self,
        ctx: Context,
        requests: BoxStream<Result<Req, ConnectError>>,
    ) -> BoxFuture<'static, Result<(Res, Context), ConnectError>>;
}

/// Wrapper that implements ClientStreamingHandler for async functions.
pub struct FnClientStreamingHandler<F> {
    f: Arc<F>,
}

impl<F> FnClientStreamingHandler<F> {
    /// Create a new function client streaming handler.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F, Fut, Req, Res> ClientStreamingHandler<Req, Res> for FnClientStreamingHandler<F>
where
    F: Fn(Context, BoxStream<Result<Req, ConnectError>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(Res, Context), ConnectError>> + Send + 'static,
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    fn call(
        &self,
        ctx: Context,
        requests: BoxStream<Result<Req, ConnectError>>,
    ) -> BoxFuture<'static, Result<(Res, Context), ConnectError>> {
        let f = Arc::clone(&self.f);
        Box::pin(async move { f(ctx, requests).await })
    }
}

/// Helper function to create a client streaming handler from an async function.
pub fn client_streaming_handler_fn<F, Fut, Req, Res>(f: F) -> FnClientStreamingHandler<F>
where
    F: Fn(Context, BoxStream<Result<Req, ConnectError>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(Res, Context), ConnectError>> + Send + 'static,
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    FnClientStreamingHandler::new(f)
}

/// Type-erased client streaming handler for use in the router.
pub(crate) trait ErasedClientStreamingHandler: Send + Sync {
    /// Handle a client streaming request with a stream of raw message bytes.
    fn call_erased(
        &self,
        ctx: Context,
        requests: BoxStream<Result<Bytes, ConnectError>>,
        format: CodecFormat,
    ) -> BoxFuture<'static, Result<(Bytes, Context), ConnectError>>;
}

/// Wrapper to erase the types from a client streaming handler.
pub(crate) struct ClientStreamingHandlerWrapper<H, Req, Res>
where
    H: ClientStreamingHandler<Req, Res>,
    Req: Message + DeserializeOwned + Send + 'static,
    Res: Message + Serialize + Send + 'static,
{
    handler: Arc<H>,
    _phantom: std::marker::PhantomData<fn(Req) -> Res>,
}

impl<H, Req, Res> ClientStreamingHandlerWrapper<H, Req, Res>
where
    H: ClientStreamingHandler<Req, Res>,
    Req: Message + DeserializeOwned + Send + 'static,
    Res: Message + Serialize + Send + 'static,
{
    /// Create a new wrapper around the given client streaming handler.
    pub fn new(handler: H) -> Self {
        Self {
            handler: Arc::new(handler),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<H, Req, Res> ErasedClientStreamingHandler for ClientStreamingHandlerWrapper<H, Req, Res>
where
    H: ClientStreamingHandler<Req, Res>,
    Req: Message + DeserializeOwned + Send + 'static,
    Res: Message + Serialize + Send + 'static,
{
    fn call_erased(
        &self,
        ctx: Context,
        requests: BoxStream<Result<Bytes, ConnectError>>,
        format: CodecFormat,
    ) -> BoxFuture<'static, Result<(Bytes, Context), ConnectError>> {
        use futures::StreamExt as _;

        let handler = Arc::clone(&self.handler);
        Box::pin(async move {
            // Map the raw bytes stream through decode to create a typed stream
            let request_stream: BoxStream<Result<Req, ConnectError>> = Box::pin(
                requests.map(move |result| result.and_then(|raw| decode_request(&raw, format))),
            );

            let (res, ctx) = handler.call(ctx, request_stream).await?;
            let response_bytes = encode_response(&res, format)?;
            Ok((response_bytes, ctx))
        })
    }
}

/// Trait for bidirectional streaming RPC handlers.
///
/// A bidi streaming handler receives a stream of request messages and returns a stream of responses.
pub trait BidiStreamingHandler<Req, Res>: Send + Sync + 'static
where
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    /// The stream type returned by this handler.
    type Stream: Stream<Item = Result<Res, ConnectError>> + Send + 'static;

    /// Handle a bidi streaming RPC request.
    fn call(
        &self,
        ctx: Context,
        requests: BoxStream<Result<Req, ConnectError>>,
    ) -> BoxFuture<'static, Result<(Self::Stream, Context), ConnectError>>;
}

/// Wrapper that implements BidiStreamingHandler for async functions.
pub struct FnBidiStreamingHandler<F> {
    f: Arc<F>,
}

impl<F> FnBidiStreamingHandler<F> {
    /// Create a new function bidi streaming handler.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F, Fut, S, Req, Res> BidiStreamingHandler<Req, Res> for FnBidiStreamingHandler<F>
where
    F: Fn(Context, BoxStream<Result<Req, ConnectError>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(S, Context), ConnectError>> + Send + 'static,
    S: Stream<Item = Result<Res, ConnectError>> + Send + 'static,
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    type Stream = S;

    fn call(
        &self,
        ctx: Context,
        requests: BoxStream<Result<Req, ConnectError>>,
    ) -> BoxFuture<'static, Result<(Self::Stream, Context), ConnectError>> {
        let f = Arc::clone(&self.f);
        Box::pin(async move { f(ctx, requests).await })
    }
}

/// Helper function to create a bidi streaming handler from an async function.
pub fn bidi_streaming_handler_fn<F, Fut, S, Req, Res>(f: F) -> FnBidiStreamingHandler<F>
where
    F: Fn(Context, BoxStream<Result<Req, ConnectError>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(S, Context), ConnectError>> + Send + 'static,
    S: Stream<Item = Result<Res, ConnectError>> + Send + 'static,
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    FnBidiStreamingHandler::new(f)
}

// ============================================================================
// View-based handler infrastructure (zero-copy request deserialization)
// ============================================================================

/// Decode a request as an `OwnedView` from bytes using the specified codec format.
///
/// For proto-encoded requests, this is a true zero-copy decode — the view borrows
/// directly from the input bytes. For JSON-encoded requests, the data is first
/// deserialized to an owned message, then re-encoded to proto bytes and decoded as
/// a view. This JSON round-trip adds overhead relative to owned-type decoding, but
/// is negligible compared to JSON parsing itself.
#[doc(hidden)] // exposed only for dispatcher::codegen (generated code)
pub fn decode_request_view<ReqView>(
    request: Bytes,
    format: CodecFormat,
) -> Result<OwnedView<ReqView>, ConnectError>
where
    ReqView: MessageView<'static> + Send,
    ReqView::Owned: Message + DeserializeOwned,
{
    match format {
        CodecFormat::Proto => OwnedView::<ReqView>::decode(request).map_err(|e| {
            ConnectError::invalid_argument(format!("failed to decode proto request: {e}"))
        }),
        CodecFormat::Json => {
            let owned: ReqView::Owned = serde_json::from_slice(&request).map_err(|e| {
                ConnectError::invalid_argument(format!("failed to decode JSON request: {e}"))
            })?;
            OwnedView::<ReqView>::from_owned(&owned)
                .map_err(|e| ConnectError::internal(format!("failed to re-encode for view: {e}")))
        }
    }
}

/// Trait for unary RPC handlers using zero-copy request views.
pub trait ViewHandler<ReqView, Res>: Send + Sync + 'static
where
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
{
    /// Handle a unary RPC request with a zero-copy view.
    fn call(
        &self,
        ctx: Context,
        request: OwnedView<ReqView>,
    ) -> BoxFuture<'static, Result<(Res, Context), ConnectError>>;
}

/// Wrapper that implements ViewHandler for async functions.
pub struct FnViewHandler<F> {
    f: Arc<F>,
}

impl<F> FnViewHandler<F> {
    /// Create a new function view handler.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F, Fut, ReqView, Res> ViewHandler<ReqView, Res> for FnViewHandler<F>
where
    F: Fn(Context, OwnedView<ReqView>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(Res, Context), ConnectError>> + Send + 'static,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
{
    fn call(
        &self,
        ctx: Context,
        request: OwnedView<ReqView>,
    ) -> BoxFuture<'static, Result<(Res, Context), ConnectError>> {
        let f = Arc::clone(&self.f);
        Box::pin(async move { f(ctx, request).await })
    }
}

/// Helper function to create a view handler from an async function.
pub fn view_handler_fn<F, Fut, ReqView, Res>(f: F) -> FnViewHandler<F>
where
    F: Fn(Context, OwnedView<ReqView>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(Res, Context), ConnectError>> + Send + 'static,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
{
    FnViewHandler::new(f)
}

/// Wrapper to erase the types from a unary view handler.
pub(crate) struct UnaryViewHandlerWrapper<H, ReqView, Res>
where
    H: ViewHandler<ReqView, Res>,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    ReqView::Owned: Message + DeserializeOwned,
    Res: Message + Serialize + Send + 'static,
{
    handler: Arc<H>,
    _phantom: std::marker::PhantomData<fn(ReqView) -> Res>,
}

impl<H, ReqView, Res> UnaryViewHandlerWrapper<H, ReqView, Res>
where
    H: ViewHandler<ReqView, Res>,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    ReqView::Owned: Message + DeserializeOwned,
    Res: Message + Serialize + Send + 'static,
{
    pub fn new(handler: H) -> Self {
        Self {
            handler: Arc::new(handler),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<H, ReqView, Res> ErasedHandler for UnaryViewHandlerWrapper<H, ReqView, Res>
where
    H: ViewHandler<ReqView, Res>,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    ReqView::Owned: Message + DeserializeOwned,
    Res: Message + Serialize + Send + 'static,
{
    fn call_erased(
        &self,
        ctx: Context,
        request: Bytes,
        format: CodecFormat,
    ) -> BoxFuture<'static, Result<(Bytes, Context), ConnectError>> {
        let handler = Arc::clone(&self.handler);
        Box::pin(async move {
            let req = decode_request_view::<ReqView>(request, format)?;
            let (res, ctx) = handler.call(ctx, req).await?;
            let response_bytes = encode_response(&res, format)?;
            Ok((response_bytes, ctx))
        })
    }

    fn is_streaming(&self) -> bool {
        false
    }
}

/// Trait for server streaming RPC handlers using zero-copy request views.
pub trait ViewStreamingHandler<ReqView, Res>: Send + Sync + 'static
where
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
{
    /// The stream type returned by this handler.
    type Stream: Stream<Item = Result<Res, ConnectError>> + Send + 'static;

    /// Handle a server streaming RPC request with a zero-copy view.
    fn call(
        &self,
        ctx: Context,
        request: OwnedView<ReqView>,
    ) -> BoxFuture<'static, Result<(Self::Stream, Context), ConnectError>>;
}

/// Wrapper that implements ViewStreamingHandler for async functions.
pub struct FnViewStreamingHandler<F> {
    f: Arc<F>,
}

impl<F> FnViewStreamingHandler<F> {
    /// Create a new function view streaming handler.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F, Fut, S, ReqView, Res> ViewStreamingHandler<ReqView, Res> for FnViewStreamingHandler<F>
where
    F: Fn(Context, OwnedView<ReqView>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(S, Context), ConnectError>> + Send + 'static,
    S: Stream<Item = Result<Res, ConnectError>> + Send + 'static,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
{
    type Stream = S;

    fn call(
        &self,
        ctx: Context,
        request: OwnedView<ReqView>,
    ) -> BoxFuture<'static, Result<(Self::Stream, Context), ConnectError>> {
        let f = Arc::clone(&self.f);
        Box::pin(async move { f(ctx, request).await })
    }
}

/// Helper function to create a view streaming handler from an async function.
pub fn view_streaming_handler_fn<F, Fut, S, ReqView, Res>(f: F) -> FnViewStreamingHandler<F>
where
    F: Fn(Context, OwnedView<ReqView>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(S, Context), ConnectError>> + Send + 'static,
    S: Stream<Item = Result<Res, ConnectError>> + Send + 'static,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
{
    FnViewStreamingHandler::new(f)
}

/// Wrapper to erase the types from a server streaming view handler.
pub(crate) struct ServerStreamingViewHandlerWrapper<H, ReqView, Res>
where
    H: ViewStreamingHandler<ReqView, Res>,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    ReqView::Owned: Message + DeserializeOwned,
    Res: Message + Serialize + Send + 'static,
{
    handler: Arc<H>,
    _phantom: std::marker::PhantomData<fn(ReqView) -> Res>,
}

impl<H, ReqView, Res> ServerStreamingViewHandlerWrapper<H, ReqView, Res>
where
    H: ViewStreamingHandler<ReqView, Res>,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    ReqView::Owned: Message + DeserializeOwned,
    Res: Message + Serialize + Send + 'static,
{
    pub fn new(handler: H) -> Self {
        Self {
            handler: Arc::new(handler),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<H, ReqView, Res> ErasedStreamingHandler for ServerStreamingViewHandlerWrapper<H, ReqView, Res>
where
    H: ViewStreamingHandler<ReqView, Res>,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    ReqView::Owned: Message + DeserializeOwned,
    Res: Message + Serialize + Send + 'static,
{
    fn call_erased(
        &self,
        ctx: Context,
        request: Bytes,
        format: CodecFormat,
    ) -> StreamingHandlerResult {
        let handler = Arc::clone(&self.handler);
        Box::pin(async move {
            let req = decode_request_view::<ReqView>(request, format)?;
            let (stream, ctx) = handler.call(ctx, req).await?;

            let encoded_stream: BoxStream<Result<Bytes, ConnectError>> = {
                use futures::StreamExt as _;
                Box::pin(
                    futures::stream::unfold(
                        (
                            Box::pin(stream)
                                as Pin<Box<dyn Stream<Item = Result<Res, ConnectError>> + Send>>,
                            format,
                        ),
                        async |(mut stream, format)| match stream.next().await {
                            Some(Ok(res)) => {
                                let encoded = encode_response(&res, format);
                                Some((encoded, (stream, format)))
                            }
                            Some(Err(e)) => Some((Err(e), (stream, format))),
                            None => None,
                        },
                    )
                    .fuse(),
                )
            };

            Ok((encoded_stream, ctx))
        })
    }
}

/// Trait for client streaming RPC handlers using zero-copy request views.
pub trait ViewClientStreamingHandler<ReqView, Res>: Send + Sync + 'static
where
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
{
    /// Handle a client streaming RPC request with zero-copy view items.
    fn call(
        &self,
        ctx: Context,
        requests: BoxStream<Result<OwnedView<ReqView>, ConnectError>>,
    ) -> BoxFuture<'static, Result<(Res, Context), ConnectError>>;
}

/// Wrapper that implements ViewClientStreamingHandler for async functions.
pub struct FnViewClientStreamingHandler<F> {
    f: Arc<F>,
}

impl<F> FnViewClientStreamingHandler<F> {
    /// Create a new function view client streaming handler.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F, Fut, ReqView, Res> ViewClientStreamingHandler<ReqView, Res>
    for FnViewClientStreamingHandler<F>
where
    F: Fn(Context, BoxStream<Result<OwnedView<ReqView>, ConnectError>>) -> Fut
        + Send
        + Sync
        + 'static,
    Fut: Future<Output = Result<(Res, Context), ConnectError>> + Send + 'static,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
{
    fn call(
        &self,
        ctx: Context,
        requests: BoxStream<Result<OwnedView<ReqView>, ConnectError>>,
    ) -> BoxFuture<'static, Result<(Res, Context), ConnectError>> {
        let f = Arc::clone(&self.f);
        Box::pin(async move { f(ctx, requests).await })
    }
}

/// Helper function to create a view client streaming handler from an async function.
pub fn view_client_streaming_handler_fn<F, Fut, ReqView, Res>(
    f: F,
) -> FnViewClientStreamingHandler<F>
where
    F: Fn(Context, BoxStream<Result<OwnedView<ReqView>, ConnectError>>) -> Fut
        + Send
        + Sync
        + 'static,
    Fut: Future<Output = Result<(Res, Context), ConnectError>> + Send + 'static,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
{
    FnViewClientStreamingHandler::new(f)
}

/// Wrapper to erase the types from a client streaming view handler.
pub(crate) struct ClientStreamingViewHandlerWrapper<H, ReqView, Res>
where
    H: ViewClientStreamingHandler<ReqView, Res>,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    ReqView::Owned: Message + DeserializeOwned,
    Res: Message + Serialize + Send + 'static,
{
    handler: Arc<H>,
    _phantom: std::marker::PhantomData<fn(ReqView) -> Res>,
}

impl<H, ReqView, Res> ClientStreamingViewHandlerWrapper<H, ReqView, Res>
where
    H: ViewClientStreamingHandler<ReqView, Res>,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    ReqView::Owned: Message + DeserializeOwned,
    Res: Message + Serialize + Send + 'static,
{
    pub fn new(handler: H) -> Self {
        Self {
            handler: Arc::new(handler),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<H, ReqView, Res> ErasedClientStreamingHandler
    for ClientStreamingViewHandlerWrapper<H, ReqView, Res>
where
    H: ViewClientStreamingHandler<ReqView, Res>,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    ReqView::Owned: Message + DeserializeOwned,
    Res: Message + Serialize + Send + 'static,
{
    fn call_erased(
        &self,
        ctx: Context,
        requests: BoxStream<Result<Bytes, ConnectError>>,
        format: CodecFormat,
    ) -> BoxFuture<'static, Result<(Bytes, Context), ConnectError>> {
        use futures::StreamExt as _;

        let handler = Arc::clone(&self.handler);
        Box::pin(async move {
            let request_stream: BoxStream<Result<OwnedView<ReqView>, ConnectError>> =
                Box::pin(requests.map(move |result| {
                    result.and_then(|raw| decode_request_view::<ReqView>(raw, format))
                }));

            let (res, ctx) = handler.call(ctx, request_stream).await?;
            let response_bytes = encode_response(&res, format)?;
            Ok((response_bytes, ctx))
        })
    }
}

/// Trait for bidi streaming RPC handlers using zero-copy request views.
pub trait ViewBidiStreamingHandler<ReqView, Res>: Send + Sync + 'static
where
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
{
    /// The stream type returned by this handler.
    type Stream: Stream<Item = Result<Res, ConnectError>> + Send + 'static;

    /// Handle a bidi streaming RPC request with zero-copy view items.
    fn call(
        &self,
        ctx: Context,
        requests: BoxStream<Result<OwnedView<ReqView>, ConnectError>>,
    ) -> BoxFuture<'static, Result<(Self::Stream, Context), ConnectError>>;
}

/// Wrapper that implements ViewBidiStreamingHandler for async functions.
pub struct FnViewBidiStreamingHandler<F> {
    f: Arc<F>,
}

impl<F> FnViewBidiStreamingHandler<F> {
    /// Create a new function view bidi streaming handler.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F, Fut, S, ReqView, Res> ViewBidiStreamingHandler<ReqView, Res>
    for FnViewBidiStreamingHandler<F>
where
    F: Fn(Context, BoxStream<Result<OwnedView<ReqView>, ConnectError>>) -> Fut
        + Send
        + Sync
        + 'static,
    Fut: Future<Output = Result<(S, Context), ConnectError>> + Send + 'static,
    S: Stream<Item = Result<Res, ConnectError>> + Send + 'static,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
{
    type Stream = S;

    fn call(
        &self,
        ctx: Context,
        requests: BoxStream<Result<OwnedView<ReqView>, ConnectError>>,
    ) -> BoxFuture<'static, Result<(Self::Stream, Context), ConnectError>> {
        let f = Arc::clone(&self.f);
        Box::pin(async move { f(ctx, requests).await })
    }
}

/// Helper function to create a view bidi streaming handler from an async function.
pub fn view_bidi_streaming_handler_fn<F, Fut, S, ReqView, Res>(
    f: F,
) -> FnViewBidiStreamingHandler<F>
where
    F: Fn(Context, BoxStream<Result<OwnedView<ReqView>, ConnectError>>) -> Fut
        + Send
        + Sync
        + 'static,
    Fut: Future<Output = Result<(S, Context), ConnectError>> + Send + 'static,
    S: Stream<Item = Result<Res, ConnectError>> + Send + 'static,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
{
    FnViewBidiStreamingHandler::new(f)
}

/// Wrapper to erase the types from a bidi streaming view handler.
pub(crate) struct BidiStreamingViewHandlerWrapper<H, ReqView, Res>
where
    H: ViewBidiStreamingHandler<ReqView, Res>,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    ReqView::Owned: Message + DeserializeOwned,
    Res: Message + Serialize + Send + 'static,
{
    handler: Arc<H>,
    _phantom: std::marker::PhantomData<fn(ReqView) -> Res>,
}

impl<H, ReqView, Res> BidiStreamingViewHandlerWrapper<H, ReqView, Res>
where
    H: ViewBidiStreamingHandler<ReqView, Res>,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    ReqView::Owned: Message + DeserializeOwned,
    Res: Message + Serialize + Send + 'static,
{
    pub fn new(handler: H) -> Self {
        Self {
            handler: Arc::new(handler),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<H, ReqView, Res> ErasedBidiStreamingHandler
    for BidiStreamingViewHandlerWrapper<H, ReqView, Res>
where
    H: ViewBidiStreamingHandler<ReqView, Res>,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    ReqView::Owned: Message + DeserializeOwned,
    Res: Message + Serialize + Send + 'static,
{
    fn call_erased(
        &self,
        ctx: Context,
        requests: BoxStream<Result<Bytes, ConnectError>>,
        format: CodecFormat,
    ) -> StreamingHandlerResult {
        use futures::StreamExt as _;

        let handler = Arc::clone(&self.handler);
        Box::pin(async move {
            let request_stream: BoxStream<Result<OwnedView<ReqView>, ConnectError>> =
                Box::pin(requests.map(move |result| {
                    result.and_then(|raw| decode_request_view::<ReqView>(raw, format))
                }));

            let (stream, ctx) = handler.call(ctx, request_stream).await?;

            let encoded_stream: BoxStream<Result<Bytes, ConnectError>> = {
                Box::pin(
                    futures::stream::unfold(
                        (
                            Box::pin(stream)
                                as Pin<Box<dyn Stream<Item = Result<Res, ConnectError>> + Send>>,
                            format,
                        ),
                        async |(mut stream, format)| match stream.next().await {
                            Some(Ok(res)) => {
                                let encoded = encode_response(&res, format);
                                Some((encoded, (stream, format)))
                            }
                            Some(Err(e)) => Some((Err(e), (stream, format))),
                            None => None,
                        },
                    )
                    .fuse(),
                )
            };

            Ok((encoded_stream, ctx))
        })
    }
}

/// Type-erased bidi streaming handler for use in the router.
pub(crate) trait ErasedBidiStreamingHandler: Send + Sync {
    /// Handle a bidi streaming request with a stream of raw message bytes.
    fn call_erased(
        &self,
        ctx: Context,
        requests: BoxStream<Result<Bytes, ConnectError>>,
        format: CodecFormat,
    ) -> StreamingHandlerResult;
}

/// Wrapper to erase the types from a bidi streaming handler.
pub(crate) struct BidiStreamingHandlerWrapper<H, Req, Res>
where
    H: BidiStreamingHandler<Req, Res>,
    Req: Message + DeserializeOwned + Send + 'static,
    Res: Message + Serialize + Send + 'static,
{
    handler: Arc<H>,
    _phantom: std::marker::PhantomData<fn(Req) -> Res>,
}

impl<H, Req, Res> BidiStreamingHandlerWrapper<H, Req, Res>
where
    H: BidiStreamingHandler<Req, Res>,
    Req: Message + DeserializeOwned + Send + 'static,
    Res: Message + Serialize + Send + 'static,
{
    /// Create a new wrapper around the given bidi streaming handler.
    pub fn new(handler: H) -> Self {
        Self {
            handler: Arc::new(handler),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<H, Req, Res> ErasedBidiStreamingHandler for BidiStreamingHandlerWrapper<H, Req, Res>
where
    H: BidiStreamingHandler<Req, Res>,
    Req: Message + DeserializeOwned + Send + 'static,
    Res: Message + Serialize + Send + 'static,
{
    fn call_erased(
        &self,
        ctx: Context,
        requests: BoxStream<Result<Bytes, ConnectError>>,
        format: CodecFormat,
    ) -> StreamingHandlerResult {
        use futures::StreamExt as _;

        let handler = Arc::clone(&self.handler);
        Box::pin(async move {
            // Map the raw bytes stream through decode to create a typed stream
            let request_stream: BoxStream<Result<Req, ConnectError>> = Box::pin(
                requests.map(move |result| result.and_then(|raw| decode_request(&raw, format))),
            );

            let (stream, ctx) = handler.call(ctx, request_stream).await?;

            // Map the stream to encode each response
            let encoded_stream: BoxStream<Result<Bytes, ConnectError>> = {
                Box::pin(
                    futures::stream::unfold(
                        (
                            Box::pin(stream)
                                as Pin<Box<dyn Stream<Item = Result<Res, ConnectError>> + Send>>,
                            format,
                        ),
                        async |(mut stream, format)| match stream.next().await {
                            Some(Ok(res)) => {
                                let encoded = encode_response(&res, format);
                                Some((encoded, (stream, format)))
                            }
                            Some(Err(e)) => Some((Err(e), (stream, format))),
                            None => None,
                        },
                    )
                    .fuse(),
                )
            };

            Ok((encoded_stream, ctx))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use buffa_types::google::protobuf::{StringValue, StringValueView};

    // ── decode_request / encode_response (owned types) ─────────────────

    #[test]
    fn test_decode_request_proto() {
        let msg = StringValue::from("hello");
        let encoded = Bytes::from(msg.encode_to_vec());
        let decoded: StringValue = decode_request(&encoded, CodecFormat::Proto).unwrap();
        assert_eq!(decoded.value, "hello");
    }

    #[test]
    fn test_decode_request_json() {
        // StringValue serializes as a bare JSON string per WKT mapping
        let encoded = Bytes::from_static(b"\"world\"");
        let decoded: StringValue = decode_request(&encoded, CodecFormat::Json).unwrap();
        assert_eq!(decoded.value, "world");
    }

    #[test]
    fn test_decode_request_proto_invalid() {
        let garbage = Bytes::from_static(&[0xFF, 0xFF, 0xFF]);
        let err = decode_request::<StringValue>(&garbage, CodecFormat::Proto).unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::InvalidArgument);
    }

    #[test]
    fn test_decode_request_json_invalid() {
        let garbage = Bytes::from_static(b"not json");
        let err = decode_request::<StringValue>(&garbage, CodecFormat::Json).unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::InvalidArgument);
    }

    #[test]
    fn test_encode_response_proto() {
        let msg = StringValue::from("reply");
        let encoded = encode_response(&msg, CodecFormat::Proto).unwrap();
        // Round-trip to verify
        let decoded = StringValue::decode_from_slice(&encoded).unwrap();
        assert_eq!(decoded.value, "reply");
    }

    #[test]
    fn test_encode_response_json() {
        let msg = StringValue::from("reply");
        let encoded = encode_response(&msg, CodecFormat::Json).unwrap();
        // StringValue serializes as a bare JSON string
        assert_eq!(&encoded[..], b"\"reply\"");
    }

    #[test]
    fn test_proto_roundtrip() {
        let msg = StringValue::from("roundtrip");
        let encoded = encode_response(&msg, CodecFormat::Proto).unwrap();
        let decoded: StringValue = decode_request(&encoded, CodecFormat::Proto).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_json_roundtrip() {
        let msg = StringValue::from("roundtrip");
        let encoded = encode_response(&msg, CodecFormat::Json).unwrap();
        let decoded: StringValue = decode_request(&encoded, CodecFormat::Json).unwrap();
        assert_eq!(decoded.value, msg.value);
    }

    // ── decode_request_view (zero-copy views) ───────────────────────────

    #[test]
    fn test_decode_request_view_proto() {
        let msg = StringValue::from("view-test");
        let encoded = Bytes::from(msg.encode_to_vec());
        let view = decode_request_view::<StringValueView>(encoded, CodecFormat::Proto).unwrap();
        // OwnedView derefs to the inner view
        assert_eq!(view.value, "view-test");
    }

    #[test]
    fn test_decode_request_view_json() {
        // JSON path: deserialize to owned, re-encode to proto, decode as view
        let encoded = Bytes::from_static(b"\"json-view\"");
        let view = decode_request_view::<StringValueView>(encoded, CodecFormat::Json).unwrap();
        assert_eq!(view.value, "json-view");
    }

    #[test]
    fn test_decode_request_view_proto_invalid() {
        let garbage = Bytes::from_static(&[0xFF, 0xFF, 0xFF]);
        let err = decode_request_view::<StringValueView>(garbage, CodecFormat::Proto).unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::InvalidArgument);
    }

    // ── Context helpers ────────────────────────────────────────────────

    #[test]
    fn test_context_new() {
        let mut headers = http::HeaderMap::new();
        headers.insert("x-custom", http::HeaderValue::from_static("value"));
        let ctx = Context::new(headers);
        assert_eq!(
            ctx.header(&http::header::HeaderName::from_static("x-custom"))
                .unwrap(),
            "value"
        );
        assert!(ctx.response_headers.is_empty());
        assert!(ctx.trailers.is_empty());
        assert!(ctx.deadline.is_none());
        assert!(ctx.compress_response.is_none());
    }

    #[test]
    fn test_context_set_trailer() {
        let mut ctx = Context::default();
        ctx.set_trailer(
            http::header::HeaderName::from_static("x-trailer"),
            http::HeaderValue::from_static("trailer-value"),
        );
        assert_eq!(ctx.trailers.get("x-trailer").unwrap(), "trailer-value");
    }

    #[test]
    fn test_context_set_compression() {
        let mut ctx = Context::default();
        ctx.set_compression(true);
        assert_eq!(ctx.compress_response, Some(true));
        ctx.set_compression(false);
        assert_eq!(ctx.compress_response, Some(false));
    }

    #[test]
    fn test_context_with_deadline() {
        // Server dispatch paths must populate deadline so handlers can
        // propagate it to downstream calls (e.g. as a grpc-timeout header).
        let now = std::time::Instant::now();
        let deadline = now + std::time::Duration::from_secs(5);
        let ctx = Context::new(http::HeaderMap::new()).with_deadline(Some(deadline));
        assert_eq!(ctx.deadline, Some(deadline));

        let ctx = Context::new(http::HeaderMap::new()).with_deadline(None);
        assert_eq!(ctx.deadline, None);
    }
}
