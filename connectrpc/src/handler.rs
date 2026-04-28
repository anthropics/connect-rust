//! Handler traits for implementing RPC methods.
//!
//! This module defines the traits that RPC method implementations must
//! satisfy. Generated `FooService` traits are the primary surface; these
//! lower-level traits exist for the [`Router`](crate::Router) path that
//! registers handlers without codegen.
//!
//! Handlers receive a read-only [`RequestContext`] and return a
//! [`Response<B>`](crate::Response) carrying the body plus any response
//! headers/trailers/compression hint. See [`crate::response`] for the
//! type definitions.
//!
//! # Why response metadata lives on `Response<B>`
//!
//! The earlier `Context` design conflated request-side reads
//! (`headers`, `deadline`, `extensions`) with response-side writes
//! (`response_headers`, `trailers`, `compress_response`) on one struct
//! that the handler took ownership of and threaded back. Splitting it
//! gives a clean in/out separation: handlers that don't touch response
//! metadata bind `_ctx` and return `Ok(body.into())` with no `mut`
//! ceremony, while handlers that do attach metadata get a fluent
//! builder (`Response::new(body).with_header(..).with_trailer(..)`)
//! instead of field-mutation followed by `Ok((body, ctx))`.

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
use crate::response::{
    Encodable, EncodedResponse, RequestContext, Response, ServiceResult, ServiceStream,
};

/// Decode a request message from bytes using the specified codec format.
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

/// Type alias for a boxed future used in handlers.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Type alias for a boxed stream of encoded response bytes.
pub type BoxStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>;

/// Map a stream of typed responses through [`Encodable`].
fn encode_body_stream<Res, S>(
    stream: S,
    format: CodecFormat,
) -> BoxStream<Result<Bytes, ConnectError>>
where
    Res: Message + Serialize + Send + 'static,
    S: Stream<Item = Result<Res, ConnectError>> + Send + 'static,
{
    use futures::StreamExt as _;
    Box::pin(
        futures::stream::unfold(
            (
                Box::pin(stream) as Pin<Box<dyn Stream<Item = Result<Res, ConnectError>> + Send>>,
                format,
            ),
            async |(mut s, fmt)| match s.next().await {
                Some(Ok(res)) => Some((Encodable::<Res>::encode(&res, fmt), (s, fmt))),
                Some(Err(e)) => Some((Err(e), (s, fmt))),
                None => None,
            },
        )
        .fuse(),
    )
}

// ============================================================================
// Type-erased handler boundaries (Router → service.rs)
// ============================================================================

/// Type-erased unary handler for use in the router.
pub(crate) trait ErasedHandler: Send + Sync {
    /// Handle a request with raw bytes and specified codec format.
    fn call_erased(
        &self,
        ctx: RequestContext,
        request: Bytes,
        format: CodecFormat,
    ) -> BoxFuture<'static, Result<EncodedResponse, ConnectError>>;

    /// Check if this is a streaming handler.
    #[allow(dead_code)]
    fn is_streaming(&self) -> bool;
}

/// Result type for erased streaming handlers.
pub(crate) type StreamingHandlerResult =
    BoxFuture<'static, Result<Response<BoxStream<Result<Bytes, ConnectError>>>, ConnectError>>;

/// Type-erased server-streaming handler for use in the router.
pub(crate) trait ErasedStreamingHandler: Send + Sync {
    /// Handle a streaming request with raw bytes and specified codec format.
    fn call_erased(
        &self,
        ctx: RequestContext,
        request: Bytes,
        format: CodecFormat,
    ) -> StreamingHandlerResult;
}

/// Type-erased client-streaming handler for use in the router.
pub(crate) trait ErasedClientStreamingHandler: Send + Sync {
    /// Handle a client streaming request with a stream of raw message bytes.
    fn call_erased(
        &self,
        ctx: RequestContext,
        requests: BoxStream<Result<Bytes, ConnectError>>,
        format: CodecFormat,
    ) -> BoxFuture<'static, Result<EncodedResponse, ConnectError>>;
}

/// Type-erased bidi-streaming handler for use in the router.
pub(crate) trait ErasedBidiStreamingHandler: Send + Sync {
    /// Handle a bidi streaming request with a stream of raw message bytes.
    fn call_erased(
        &self,
        ctx: RequestContext,
        requests: BoxStream<Result<Bytes, ConnectError>>,
        format: CodecFormat,
    ) -> StreamingHandlerResult;
}

// ============================================================================
// Unary handler (owned request)
// ============================================================================

/// Trait for unary RPC handlers (owned request type).
///
/// Handlers return a [`Response<Self::Body>`](crate::Response) where
/// `Body` is any type [`Encodable`] as `Res` — typically `Res` itself.
/// The happy path is `Ok(res.into())`.
pub trait Handler<Req, Res>: Send + Sync + 'static
where
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    /// The response body type. Typically `Res`; the [`Encodable`] bound
    /// lets handlers return a borrowing view in a follow-up.
    type Body: Encodable<Res> + Send + 'static;

    /// Handle a unary RPC request.
    fn call(
        &self,
        ctx: RequestContext,
        request: Req,
    ) -> BoxFuture<'static, ServiceResult<Self::Body>>;
}

/// Wrapper that implements [`Handler`] for async functions.
pub struct FnHandler<F> {
    f: Arc<F>,
}

impl<F> FnHandler<F> {
    /// Create a new function handler.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F, Fut, Req, Res, B> Handler<Req, Res> for FnHandler<F>
where
    F: Fn(RequestContext, Req) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ServiceResult<B>> + Send + 'static,
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
    B: Encodable<Res> + Send + 'static,
{
    type Body = B;

    fn call(&self, ctx: RequestContext, request: Req) -> BoxFuture<'static, ServiceResult<B>> {
        let f = Arc::clone(&self.f);
        Box::pin(async move { f(ctx, request).await })
    }
}

/// Helper function to create a handler from an async function.
pub fn handler_fn<F, Fut, Req, Res, B>(f: F) -> FnHandler<F>
where
    F: Fn(RequestContext, Req) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ServiceResult<B>> + Send + 'static,
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
    B: Encodable<Res> + Send + 'static,
{
    FnHandler::new(f)
}

/// Wrapper to erase the types from a unary handler.
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
        ctx: RequestContext,
        request: Bytes,
        format: CodecFormat,
    ) -> BoxFuture<'static, Result<EncodedResponse, ConnectError>> {
        let handler = Arc::clone(&self.handler);
        Box::pin(async move {
            let req: Req = decode_request(&request, format)?;
            handler.call(ctx, req).await?.encode::<Res>(format)
        })
    }

    fn is_streaming(&self) -> bool {
        false
    }
}

// ============================================================================
// Server-streaming handler (owned request)
// ============================================================================

/// Trait for server streaming RPC handlers.
///
/// Stream items are the owned `Res` (view-out for streams is a follow-up).
pub trait StreamingHandler<Req, Res>: Send + Sync + 'static
where
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    /// Handle a server streaming RPC request.
    fn call(
        &self,
        ctx: RequestContext,
        request: Req,
    ) -> BoxFuture<'static, ServiceResult<ServiceStream<Res>>>;
}

/// Wrapper that implements [`StreamingHandler`] for async functions.
pub struct FnStreamingHandler<F> {
    f: Arc<F>,
}

impl<F> FnStreamingHandler<F> {
    /// Create a new function streaming handler.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F, Fut, Req, Res> StreamingHandler<Req, Res> for FnStreamingHandler<F>
where
    F: Fn(RequestContext, Req) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ServiceResult<ServiceStream<Res>>> + Send + 'static,
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    fn call(
        &self,
        ctx: RequestContext,
        request: Req,
    ) -> BoxFuture<'static, ServiceResult<ServiceStream<Res>>> {
        let f = Arc::clone(&self.f);
        Box::pin(async move { f(ctx, request).await })
    }
}

/// Helper function to create a streaming handler from an async function.
pub fn streaming_handler_fn<F, Fut, Req, Res>(f: F) -> FnStreamingHandler<F>
where
    F: Fn(RequestContext, Req) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ServiceResult<ServiceStream<Res>>> + Send + 'static,
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    FnStreamingHandler::new(f)
}

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
        ctx: RequestContext,
        request: Bytes,
        format: CodecFormat,
    ) -> StreamingHandlerResult {
        let handler = Arc::clone(&self.handler);
        Box::pin(async move {
            let req: Req = decode_request(&request, format)?;
            let resp = handler.call(ctx, req).await?;
            Ok(resp.map_body(|s| encode_body_stream(s, format)))
        })
    }
}

// ============================================================================
// Client-streaming handler (owned request)
// ============================================================================

/// Trait for client streaming RPC handlers.
pub trait ClientStreamingHandler<Req, Res>: Send + Sync + 'static
where
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    /// The response body type. Typically `Res`.
    type Body: Encodable<Res> + Send + 'static;

    /// Handle a client streaming RPC request.
    fn call(
        &self,
        ctx: RequestContext,
        requests: ServiceStream<Req>,
    ) -> BoxFuture<'static, ServiceResult<Self::Body>>;
}

/// Wrapper that implements [`ClientStreamingHandler`] for async functions.
pub struct FnClientStreamingHandler<F> {
    f: Arc<F>,
}

impl<F> FnClientStreamingHandler<F> {
    /// Create a new function client streaming handler.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F, Fut, Req, Res, B> ClientStreamingHandler<Req, Res> for FnClientStreamingHandler<F>
where
    F: Fn(RequestContext, ServiceStream<Req>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ServiceResult<B>> + Send + 'static,
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
    B: Encodable<Res> + Send + 'static,
{
    type Body = B;

    fn call(
        &self,
        ctx: RequestContext,
        requests: ServiceStream<Req>,
    ) -> BoxFuture<'static, ServiceResult<B>> {
        let f = Arc::clone(&self.f);
        Box::pin(async move { f(ctx, requests).await })
    }
}

/// Helper function to create a client streaming handler from an async function.
pub fn client_streaming_handler_fn<F, Fut, Req, Res, B>(f: F) -> FnClientStreamingHandler<F>
where
    F: Fn(RequestContext, ServiceStream<Req>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ServiceResult<B>> + Send + 'static,
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
    B: Encodable<Res> + Send + 'static,
{
    FnClientStreamingHandler::new(f)
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
        ctx: RequestContext,
        requests: BoxStream<Result<Bytes, ConnectError>>,
        format: CodecFormat,
    ) -> BoxFuture<'static, Result<EncodedResponse, ConnectError>> {
        use futures::StreamExt as _;
        let handler = Arc::clone(&self.handler);
        Box::pin(async move {
            let request_stream: ServiceStream<Req> = Box::pin(
                requests.map(move |result| result.and_then(|raw| decode_request(&raw, format))),
            );
            handler
                .call(ctx, request_stream)
                .await?
                .encode::<Res>(format)
        })
    }
}

// ============================================================================
// Bidi-streaming handler (owned request)
// ============================================================================

/// Trait for bidirectional streaming RPC handlers.
pub trait BidiStreamingHandler<Req, Res>: Send + Sync + 'static
where
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    /// Handle a bidi streaming RPC request.
    fn call(
        &self,
        ctx: RequestContext,
        requests: ServiceStream<Req>,
    ) -> BoxFuture<'static, ServiceResult<ServiceStream<Res>>>;
}

/// Wrapper that implements [`BidiStreamingHandler`] for async functions.
pub struct FnBidiStreamingHandler<F> {
    f: Arc<F>,
}

impl<F> FnBidiStreamingHandler<F> {
    /// Create a new function bidi streaming handler.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F, Fut, Req, Res> BidiStreamingHandler<Req, Res> for FnBidiStreamingHandler<F>
where
    F: Fn(RequestContext, ServiceStream<Req>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ServiceResult<ServiceStream<Res>>> + Send + 'static,
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    fn call(
        &self,
        ctx: RequestContext,
        requests: ServiceStream<Req>,
    ) -> BoxFuture<'static, ServiceResult<ServiceStream<Res>>> {
        let f = Arc::clone(&self.f);
        Box::pin(async move { f(ctx, requests).await })
    }
}

/// Helper function to create a bidi streaming handler from an async function.
pub fn bidi_streaming_handler_fn<F, Fut, Req, Res>(f: F) -> FnBidiStreamingHandler<F>
where
    F: Fn(RequestContext, ServiceStream<Req>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ServiceResult<ServiceStream<Res>>> + Send + 'static,
    Req: Message + Send + 'static,
    Res: Message + Send + 'static,
{
    FnBidiStreamingHandler::new(f)
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
        ctx: RequestContext,
        requests: BoxStream<Result<Bytes, ConnectError>>,
        format: CodecFormat,
    ) -> StreamingHandlerResult {
        use futures::StreamExt as _;
        let handler = Arc::clone(&self.handler);
        Box::pin(async move {
            let request_stream: ServiceStream<Req> = Box::pin(
                requests.map(move |result| result.and_then(|raw| decode_request(&raw, format))),
            );
            let resp = handler.call(ctx, request_stream).await?;
            Ok(resp.map_body(|s| encode_body_stream(s, format)))
        })
    }
}

// ============================================================================
// View-based handlers (zero-copy request views)
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
    /// The response body type. Typically `Res`.
    type Body: Encodable<Res> + Send + 'static;

    /// Handle a unary RPC request with a zero-copy view.
    fn call(
        &self,
        ctx: RequestContext,
        request: OwnedView<ReqView>,
    ) -> BoxFuture<'static, ServiceResult<Self::Body>>;
}

/// Wrapper that implements [`ViewHandler`] for async functions.
pub struct FnViewHandler<F> {
    f: Arc<F>,
}

impl<F> FnViewHandler<F> {
    /// Create a new function view handler.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F, Fut, ReqView, Res, B> ViewHandler<ReqView, Res> for FnViewHandler<F>
where
    F: Fn(RequestContext, OwnedView<ReqView>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ServiceResult<B>> + Send + 'static,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
    B: Encodable<Res> + Send + 'static,
{
    type Body = B;

    fn call(
        &self,
        ctx: RequestContext,
        request: OwnedView<ReqView>,
    ) -> BoxFuture<'static, ServiceResult<B>> {
        let f = Arc::clone(&self.f);
        Box::pin(async move { f(ctx, request).await })
    }
}

/// Helper function to create a view handler from an async function.
pub fn view_handler_fn<F, Fut, ReqView, Res, B>(f: F) -> FnViewHandler<F>
where
    F: Fn(RequestContext, OwnedView<ReqView>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ServiceResult<B>> + Send + 'static,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
    B: Encodable<Res> + Send + 'static,
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
        ctx: RequestContext,
        request: Bytes,
        format: CodecFormat,
    ) -> BoxFuture<'static, Result<EncodedResponse, ConnectError>> {
        let handler = Arc::clone(&self.handler);
        Box::pin(async move {
            let req = decode_request_view::<ReqView>(request, format)?;
            handler.call(ctx, req).await?.encode::<Res>(format)
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
    /// Handle a server streaming RPC request with a zero-copy view.
    fn call(
        &self,
        ctx: RequestContext,
        request: OwnedView<ReqView>,
    ) -> BoxFuture<'static, ServiceResult<ServiceStream<Res>>>;
}

/// Wrapper that implements [`ViewStreamingHandler`] for async functions.
pub struct FnViewStreamingHandler<F> {
    f: Arc<F>,
}

impl<F> FnViewStreamingHandler<F> {
    /// Create a new function view streaming handler.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F, Fut, ReqView, Res> ViewStreamingHandler<ReqView, Res> for FnViewStreamingHandler<F>
where
    F: Fn(RequestContext, OwnedView<ReqView>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ServiceResult<ServiceStream<Res>>> + Send + 'static,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
{
    fn call(
        &self,
        ctx: RequestContext,
        request: OwnedView<ReqView>,
    ) -> BoxFuture<'static, ServiceResult<ServiceStream<Res>>> {
        let f = Arc::clone(&self.f);
        Box::pin(async move { f(ctx, request).await })
    }
}

/// Helper function to create a view streaming handler from an async function.
pub fn view_streaming_handler_fn<F, Fut, ReqView, Res>(f: F) -> FnViewStreamingHandler<F>
where
    F: Fn(RequestContext, OwnedView<ReqView>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ServiceResult<ServiceStream<Res>>> + Send + 'static,
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
        ctx: RequestContext,
        request: Bytes,
        format: CodecFormat,
    ) -> StreamingHandlerResult {
        let handler = Arc::clone(&self.handler);
        Box::pin(async move {
            let req = decode_request_view::<ReqView>(request, format)?;
            let resp = handler.call(ctx, req).await?;
            Ok(resp.map_body(|s| encode_body_stream(s, format)))
        })
    }
}

/// Trait for client streaming RPC handlers using zero-copy request views.
pub trait ViewClientStreamingHandler<ReqView, Res>: Send + Sync + 'static
where
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
{
    /// The response body type. Typically `Res`.
    type Body: Encodable<Res> + Send + 'static;

    /// Handle a client streaming RPC request with zero-copy view items.
    fn call(
        &self,
        ctx: RequestContext,
        requests: ServiceStream<OwnedView<ReqView>>,
    ) -> BoxFuture<'static, ServiceResult<Self::Body>>;
}

/// Wrapper that implements [`ViewClientStreamingHandler`] for async functions.
pub struct FnViewClientStreamingHandler<F> {
    f: Arc<F>,
}

impl<F> FnViewClientStreamingHandler<F> {
    /// Create a new function view client streaming handler.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F, Fut, ReqView, Res, B> ViewClientStreamingHandler<ReqView, Res>
    for FnViewClientStreamingHandler<F>
where
    F: Fn(RequestContext, ServiceStream<OwnedView<ReqView>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ServiceResult<B>> + Send + 'static,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
    B: Encodable<Res> + Send + 'static,
{
    type Body = B;

    fn call(
        &self,
        ctx: RequestContext,
        requests: ServiceStream<OwnedView<ReqView>>,
    ) -> BoxFuture<'static, ServiceResult<B>> {
        let f = Arc::clone(&self.f);
        Box::pin(async move { f(ctx, requests).await })
    }
}

/// Helper function to create a view client streaming handler from an async function.
pub fn view_client_streaming_handler_fn<F, Fut, ReqView, Res, B>(
    f: F,
) -> FnViewClientStreamingHandler<F>
where
    F: Fn(RequestContext, ServiceStream<OwnedView<ReqView>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ServiceResult<B>> + Send + 'static,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
    B: Encodable<Res> + Send + 'static,
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
        ctx: RequestContext,
        requests: BoxStream<Result<Bytes, ConnectError>>,
        format: CodecFormat,
    ) -> BoxFuture<'static, Result<EncodedResponse, ConnectError>> {
        use futures::StreamExt as _;
        let handler = Arc::clone(&self.handler);
        Box::pin(async move {
            let request_stream: ServiceStream<OwnedView<ReqView>> =
                Box::pin(requests.map(move |result| {
                    result.and_then(|raw| decode_request_view::<ReqView>(raw, format))
                }));
            handler
                .call(ctx, request_stream)
                .await?
                .encode::<Res>(format)
        })
    }
}

/// Trait for bidi streaming RPC handlers using zero-copy request views.
pub trait ViewBidiStreamingHandler<ReqView, Res>: Send + Sync + 'static
where
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
{
    /// Handle a bidi streaming RPC request with zero-copy view items.
    fn call(
        &self,
        ctx: RequestContext,
        requests: ServiceStream<OwnedView<ReqView>>,
    ) -> BoxFuture<'static, ServiceResult<ServiceStream<Res>>>;
}

/// Wrapper that implements [`ViewBidiStreamingHandler`] for async functions.
pub struct FnViewBidiStreamingHandler<F> {
    f: Arc<F>,
}

impl<F> FnViewBidiStreamingHandler<F> {
    /// Create a new function view bidi streaming handler.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F, Fut, ReqView, Res> ViewBidiStreamingHandler<ReqView, Res> for FnViewBidiStreamingHandler<F>
where
    F: Fn(RequestContext, ServiceStream<OwnedView<ReqView>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ServiceResult<ServiceStream<Res>>> + Send + 'static,
    ReqView: MessageView<'static> + Send + Sync + 'static,
    Res: Message + Send + 'static,
{
    fn call(
        &self,
        ctx: RequestContext,
        requests: ServiceStream<OwnedView<ReqView>>,
    ) -> BoxFuture<'static, ServiceResult<ServiceStream<Res>>> {
        let f = Arc::clone(&self.f);
        Box::pin(async move { f(ctx, requests).await })
    }
}

/// Helper function to create a view bidi streaming handler from an async function.
pub fn view_bidi_streaming_handler_fn<F, Fut, ReqView, Res>(f: F) -> FnViewBidiStreamingHandler<F>
where
    F: Fn(RequestContext, ServiceStream<OwnedView<ReqView>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ServiceResult<ServiceStream<Res>>> + Send + 'static,
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
        ctx: RequestContext,
        requests: BoxStream<Result<Bytes, ConnectError>>,
        format: CodecFormat,
    ) -> StreamingHandlerResult {
        use futures::StreamExt as _;
        let handler = Arc::clone(&self.handler);
        Box::pin(async move {
            let request_stream: ServiceStream<OwnedView<ReqView>> =
                Box::pin(requests.map(move |result| {
                    result.and_then(|raw| decode_request_view::<ReqView>(raw, format))
                }));
            let resp = handler.call(ctx, request_stream).await?;
            Ok(resp.map_body(|s| encode_body_stream(s, format)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use buffa_types::google::protobuf::__buffa::view::StringValueView;
    use buffa_types::google::protobuf::StringValue;

    #[test]
    fn test_decode_request_proto() {
        let msg = StringValue::from("hello");
        let encoded = Bytes::from(msg.encode_to_vec());
        let decoded: StringValue = decode_request(&encoded, CodecFormat::Proto).unwrap();
        assert_eq!(decoded.value, "hello");
    }

    #[test]
    fn test_decode_request_json() {
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
    fn test_decode_request_view_proto() {
        let msg = StringValue::from("view-test");
        let encoded = Bytes::from(msg.encode_to_vec());
        let view = decode_request_view::<StringValueView>(encoded, CodecFormat::Proto).unwrap();
        assert_eq!(view.value, "view-test");
    }

    #[test]
    fn test_decode_request_view_json() {
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
}
