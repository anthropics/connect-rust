//! Request-to-handler dispatch abstraction.
//!
//! This module defines the [`Dispatcher`] trait, which replaces the
//! concrete `Arc<Router>` in [`ConnectRpcService`](crate::ConnectRpcService)
//! with a generic boundary. Two implementations are provided out of the box:
//!
//! - [`Router`](crate::Router) implements `Dispatcher` dynamically via
//!   `HashMap<String, Arc<dyn ErasedHandler>>`. This is the default and
//!   remains backward compatible with all existing code.
//! - Codegen-emitted `FooServiceServer<T>` structs implement `Dispatcher`
//!   monomorphically via a compile-time `match` on method name, with no
//!   trait objects or hash lookups in the hot path.
//!
//! The split between [`lookup`](Dispatcher::lookup) and the `call_*` methods
//! reflects the request-handling control flow in `service.rs`: first
//! determine the method *kind* (unary / server-streaming / client-streaming
//! / bidi-streaming) to select the correct body-processing path, then call
//! the handler once the body is ready.

use bytes::Bytes;

use crate::codec::CodecFormat;
use crate::error::ConnectError;
use crate::handler::BoxFuture;
use crate::handler::BoxStream;
use crate::handler::Context;
use crate::router::MethodKind;

/// Description of a method returned by [`Dispatcher::lookup`].
///
/// Carries only enough information to select the correct body-processing
/// path in `handle_request`; the actual handler invocation happens in a
/// separate `call_*` step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodDescriptor {
    /// The kind of RPC method.
    pub kind: MethodKind,
    /// Whether the method has no side effects and is eligible for Connect GET.
    ///
    /// Only meaningful for `MethodKind::Unary`. Always `false` for streaming.
    pub idempotent: bool,
}

impl MethodDescriptor {
    /// Convenience constructor for unary methods.
    #[inline]
    pub const fn unary(idempotent: bool) -> Self {
        Self {
            kind: MethodKind::Unary,
            idempotent,
        }
    }

    /// Convenience constructor for server-streaming methods.
    #[inline]
    pub const fn server_streaming() -> Self {
        Self {
            kind: MethodKind::ServerStreaming,
            idempotent: false,
        }
    }

    /// Convenience constructor for client-streaming methods.
    #[inline]
    pub const fn client_streaming() -> Self {
        Self {
            kind: MethodKind::ClientStreaming,
            idempotent: false,
        }
    }

    /// Convenience constructor for bidirectional streaming methods.
    #[inline]
    pub const fn bidi_streaming() -> Self {
        Self {
            kind: MethodKind::BidiStreaming,
            idempotent: false,
        }
    }
}

/// Result type for unary and client-streaming handler calls.
pub type UnaryResult = BoxFuture<'static, Result<(Bytes, Context), ConnectError>>;

/// Result type for server-streaming and bidi-streaming handler calls.
pub type StreamingResult =
    BoxFuture<'static, Result<(BoxStream<Result<Bytes, ConnectError>>, Context), ConnectError>>;

/// A stream of raw request message bytes (client-streaming / bidi input).
pub type RequestStream = BoxStream<Result<Bytes, ConnectError>>;

/// Method-path-to-handler dispatch.
///
/// [`ConnectRpcService`](crate::ConnectRpcService) is generic over this
/// trait. The default implementation is [`Router`](crate::Router), which
/// stores handlers in a `HashMap` behind `Arc<dyn ErasedHandler>` trait
/// objects.
///
/// Code-generated `FooServiceServer<T>` structs provide a faster
/// implementation with a compile-time `match` and no trait objects. Use
/// [`Chain`] to compose multiple services.
///
/// # Contract
///
/// The `call_*` methods assume the caller has already checked
/// [`lookup`](Dispatcher::lookup) and is invoking the variant that matches
/// the returned [`MethodKind`]. If the path is not found or the kind does
/// not match, the call methods return an `Unimplemented` error future —
/// never panic.
pub trait Dispatcher: Send + Sync + 'static {
    /// Look up a method by its `service_name/method_name` path.
    ///
    /// Returns `None` if the path is not registered.
    fn lookup(&self, path: &str) -> Option<MethodDescriptor>;

    /// Dispatch a unary call.
    ///
    /// The caller decodes the request body to raw bytes (after envelope
    /// stripping / decompression), and the dispatcher decodes it to the
    /// concrete request type, invokes the handler, and encodes the response.
    fn call_unary(
        &self,
        path: &str,
        ctx: Context,
        request: Bytes,
        format: CodecFormat,
    ) -> UnaryResult;

    /// Dispatch a server-streaming call.
    ///
    /// Single request in, stream of responses out.
    fn call_server_streaming(
        &self,
        path: &str,
        ctx: Context,
        request: Bytes,
        format: CodecFormat,
    ) -> StreamingResult;

    /// Dispatch a client-streaming call.
    ///
    /// Stream of requests in, single response out.
    fn call_client_streaming(
        &self,
        path: &str,
        ctx: Context,
        requests: RequestStream,
        format: CodecFormat,
    ) -> UnaryResult;

    /// Dispatch a bidi-streaming call.
    ///
    /// Stream of requests in, stream of responses out.
    fn call_bidi_streaming(
        &self,
        path: &str,
        ctx: Context,
        requests: RequestStream,
        format: CodecFormat,
    ) -> StreamingResult;
}

/// Return an `Unimplemented` error future for a miss in a `call_*` method.
///
/// Used by both the `Router` impl and generated code when the path
/// doesn't match the expected kind.
#[inline]
#[doc(hidden)] // exposed for generated code via codegen::
pub fn unimplemented_unary(path: &str) -> UnaryResult {
    let err = ConnectError::unimplemented(format!("method not found: {path}"));
    Box::pin(async move { Err(err) })
}

/// Return an `Unimplemented` error future for a miss in a streaming call.
#[inline]
#[doc(hidden)] // exposed for generated code via codegen::
pub fn unimplemented_streaming(path: &str) -> StreamingResult {
    let err = ConnectError::unimplemented(format!("method not found: {path}"));
    Box::pin(async move { Err(err) })
}

// ============================================================================
// Chain combinator
// ============================================================================

/// Combine two dispatchers, trying the first then falling through to the
/// second on `NotFound`.
///
/// Use this to serve multiple code-generated `FooServiceServer<T>` structs
/// from a single [`ConnectRpcService`](crate::ConnectRpcService):
///
/// ```rust,ignore
/// use connectrpc::{ConnectRpcService, Chain};
///
/// let service = ConnectRpcService::new(
///     Chain(
///         FortuneServiceServer::new(fortune_impl),
///         Chain(
///             BenchServiceServer::new(bench_impl),
///             GreetServiceServer::new(greet_impl),
///         ),
///     ),
/// );
/// ```
///
/// For more than ~5 services, prefer [`Router`](crate::Router) — the
/// linear fallthrough cost scales with chain depth.
#[derive(Clone)]
pub struct Chain<A, B>(pub A, pub B);

impl<A: Dispatcher, B: Dispatcher> Dispatcher for Chain<A, B> {
    #[inline]
    fn lookup(&self, path: &str) -> Option<MethodDescriptor> {
        self.0.lookup(path).or_else(|| self.1.lookup(path))
    }

    fn call_unary(
        &self,
        path: &str,
        ctx: Context,
        request: Bytes,
        format: CodecFormat,
    ) -> UnaryResult {
        if self.0.lookup(path).is_some() {
            self.0.call_unary(path, ctx, request, format)
        } else {
            self.1.call_unary(path, ctx, request, format)
        }
    }

    fn call_server_streaming(
        &self,
        path: &str,
        ctx: Context,
        request: Bytes,
        format: CodecFormat,
    ) -> StreamingResult {
        if self.0.lookup(path).is_some() {
            self.0.call_server_streaming(path, ctx, request, format)
        } else {
            self.1.call_server_streaming(path, ctx, request, format)
        }
    }

    fn call_client_streaming(
        &self,
        path: &str,
        ctx: Context,
        requests: RequestStream,
        format: CodecFormat,
    ) -> UnaryResult {
        if self.0.lookup(path).is_some() {
            self.0.call_client_streaming(path, ctx, requests, format)
        } else {
            self.1.call_client_streaming(path, ctx, requests, format)
        }
    }

    fn call_bidi_streaming(
        &self,
        path: &str,
        ctx: Context,
        requests: RequestStream,
        format: CodecFormat,
    ) -> StreamingResult {
        if self.0.lookup(path).is_some() {
            self.0.call_bidi_streaming(path, ctx, requests, format)
        } else {
            self.1.call_bidi_streaming(path, ctx, requests, format)
        }
    }
}

// ============================================================================
// Codegen support — NOT public API
// ============================================================================

/// Helpers for code-generated `Dispatcher` implementations.
///
/// **This module is not part of the public API.** It exists solely so that
/// `protoc-gen-connect-rust` can emit compact dispatch arms without
/// replicating stream-adapter boilerplate. Items here may change or vanish
/// between minor versions without a breaking-change notice.
#[doc(hidden)]
pub mod codegen {
    use std::pin::Pin;

    use buffa::Message;
    use buffa::view::MessageView;
    use buffa::view::OwnedView;
    use bytes::Bytes;
    use futures::Stream;
    use futures::StreamExt;
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use crate::codec::CodecFormat;
    use crate::error::ConnectError;
    use crate::handler::BoxStream;

    // Re-exports that generated code needs direct access to.
    pub use crate::handler::BoxFuture;
    pub use crate::handler::decode_request_view;
    pub use crate::handler::encode_response;

    pub use super::MethodDescriptor;
    pub use super::RequestStream;
    pub use super::StreamingResult;
    pub use super::UnaryResult;
    pub use super::unimplemented_streaming;
    pub use super::unimplemented_unary;

    /// Map a stream of typed responses through `encode_response`.
    ///
    /// Used by generated `call_server_streaming` and `call_bidi_streaming`
    /// arms to convert the handler's `Stream<Item = Result<Res, _>>` into
    /// the `Stream<Item = Result<Bytes, _>>` that the dispatcher protocol
    /// requires.
    pub fn encode_response_stream<Res, S>(
        stream: S,
        format: CodecFormat,
    ) -> BoxStream<Result<Bytes, ConnectError>>
    where
        Res: Message + Serialize + Send + 'static,
        S: Stream<Item = Result<Res, ConnectError>> + Send + 'static,
    {
        Box::pin(
            futures::stream::unfold(
                (
                    Box::pin(stream)
                        as Pin<Box<dyn Stream<Item = Result<Res, ConnectError>> + Send>>,
                    format,
                ),
                async |(mut s, fmt)| match s.next().await {
                    Some(Ok(res)) => Some((encode_response(&res, fmt), (s, fmt))),
                    Some(Err(e)) => Some((Err(e), (s, fmt))),
                    None => None,
                },
            )
            .fuse(),
        )
    }

    /// Map a stream of raw request bytes through `decode_request_view`.
    ///
    /// Used by generated `call_client_streaming` and `call_bidi_streaming`
    /// arms to convert the dispatcher's `Stream<Item = Result<Bytes, _>>`
    /// into the typed view stream the handler expects.
    pub fn decode_view_request_stream<ReqView>(
        requests: BoxStream<Result<Bytes, ConnectError>>,
        format: CodecFormat,
    ) -> BoxStream<Result<OwnedView<ReqView>, ConnectError>>
    where
        ReqView: MessageView<'static> + Send + Sync + 'static,
        ReqView::Owned: Message + DeserializeOwned,
    {
        Box::pin(
            requests.map(move |r| r.and_then(|raw| decode_request_view::<ReqView>(raw, format))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_descriptor_constructors() {
        let u = MethodDescriptor::unary(false);
        assert_eq!(u.kind, MethodKind::Unary);
        assert!(!u.idempotent);

        let ui = MethodDescriptor::unary(true);
        assert!(ui.idempotent);

        assert_eq!(
            MethodDescriptor::server_streaming().kind,
            MethodKind::ServerStreaming
        );
        assert_eq!(
            MethodDescriptor::client_streaming().kind,
            MethodKind::ClientStreaming
        );
        assert_eq!(
            MethodDescriptor::bidi_streaming().kind,
            MethodKind::BidiStreaming
        );
    }
}
