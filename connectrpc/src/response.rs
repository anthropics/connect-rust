//! Handler request/response types.
//!
//! This module splits the old `Context` struct into a read-only
//! [`RequestContext`] (passed *into* handlers) and a [`Response<B>`]
//! wrapper (returned *from* handlers). The body type `B` is bounded by
//! [`Encodable<M>`] in the generated trait so handlers can return either
//! the owned message `M` or, in a follow-up, a borrowing view.

use std::pin::Pin;
use std::time::Instant;

use buffa::Message;
use bytes::Bytes;
use futures::Stream;
use http::HeaderMap;
use http::header::{HeaderName, HeaderValue};
use serde::Serialize;

use crate::codec::CodecFormat;
use crate::error::ConnectError;

// ---------------------------------------------------------------------------
// RequestContext
// ---------------------------------------------------------------------------

/// Read-only request context passed to RPC handlers.
///
/// Carries the request headers, parsed deadline, and any
/// connection-scoped extensions (peer address, TLS certs, auth context)
/// inserted by a tower layer in front of the service. Handlers do *not*
/// return this; response-side metadata lives on [`Response`].
#[derive(Debug, Clone, Default)]
pub struct RequestContext {
    /// Request headers (after protocol-prefix stripping).
    pub headers: HeaderMap,
    /// Absolute request deadline parsed from the protocol's timeout header,
    /// if any. Propagate to downstream calls.
    pub deadline: Option<Instant>,
    /// Request extensions carried from the underlying `http::Request`.
    ///
    /// This is the passthrough for connection-scoped metadata that a
    /// tower layer in front of the service can attach — TLS peer
    /// certificates, remote socket address, auth context, etc. The
    /// dispatch path moves `parts.extensions` here verbatim; handlers
    /// read it with `ctx.extensions.get::<T>()`.
    pub extensions: http::Extensions,
}

impl RequestContext {
    /// Create a new context with the given request headers.
    pub fn new(headers: HeaderMap) -> Self {
        Self {
            headers,
            deadline: None,
            extensions: http::Extensions::new(),
        }
    }

    /// Set the request deadline (absolute `Instant`).
    #[must_use]
    pub fn with_deadline(mut self, deadline: Option<Instant>) -> Self {
        self.deadline = deadline;
        self
    }

    /// Attach request extensions captured from the underlying `http::Request`.
    #[must_use]
    pub fn with_extensions(mut self, extensions: http::Extensions) -> Self {
        self.extensions = extensions;
        self
    }

    /// Get a request header value.
    pub fn header(&self, key: &HeaderName) -> Option<&HeaderValue> {
        self.headers.get(key)
    }
}

// ---------------------------------------------------------------------------
// Response<B>
// ---------------------------------------------------------------------------

/// Handler response wrapper: a body plus optional response headers,
/// trailers, and compression hint.
///
/// `B` is bounded by [`Encodable<M>`] in the generated service trait so
/// handlers can return the owned message `M` (the common case), or any
/// type that encodes to the same wire bytes.
///
/// # Happy path
///
/// `From<B>` makes `Ok(body.into())` the shortest spelling:
///
/// ```rust,ignore
/// async fn say(&self, _ctx: RequestContext, req: OwnedSayRequestView)
///     -> ServiceResult<SayResponse>
/// {
///     Ok(SayResponse { sentence: reply, ..Default::default() }.into())
/// }
/// ```
///
/// # With metadata
///
/// ```rust,ignore
/// Ok(Response::new(reply)
///     .with_header("x-request-id", id)
///     .with_trailer("x-timing", elapsed))
/// ```
#[derive(Debug, Clone)]
pub struct Response<B> {
    /// The response body.
    pub body: B,
    /// Response headers to send before the body.
    pub headers: HeaderMap,
    /// Trailers to send after the body. Sent as HTTP/2 trailing
    /// HEADERS for gRPC, or as `trailer-`-prefixed headers / the
    /// EndStreamResponse JSON for Connect.
    pub trailers: HeaderMap,
    /// Whether to compress the response. `None` uses the server's
    /// compression policy; `Some(false)` disables compression for this
    /// response, `Some(true)` forces it.
    pub compress: Option<bool>,
}

impl<B> Response<B> {
    /// Wrap a body with empty response metadata.
    pub fn new(body: B) -> Self {
        Self {
            body,
            headers: HeaderMap::new(),
            trailers: HeaderMap::new(),
            compress: None,
        }
    }

    /// Append a response header.
    ///
    /// Uses [`HeaderMap::append`], so calling twice with the same name
    /// accumulates values rather than replacing.
    ///
    /// # Panics
    ///
    /// Panics if `name` or `value` cannot be converted into the
    /// corresponding header type (invalid characters, non-ASCII name,
    /// etc.). Use [`try_with_header`](Self::try_with_header) for
    /// dynamic values, or the `headers` field directly for full
    /// control.
    #[must_use]
    pub fn with_header<K, V>(mut self, name: K, value: V) -> Self
    where
        K: TryInto<HeaderName>,
        K::Error: std::fmt::Debug,
        V: TryInto<HeaderValue>,
        V::Error: std::fmt::Debug,
    {
        self.headers
            .append(name.try_into().unwrap(), value.try_into().unwrap());
        self
    }

    /// Append a response header, returning an error if `name` or
    /// `value` is invalid.
    ///
    /// Non-panicking sibling of [`with_header`](Self::with_header) for
    /// dynamic values. Uses [`HeaderMap::append`], so repeated calls
    /// accumulate.
    pub fn try_with_header<K, V>(mut self, name: K, value: V) -> Result<Self, http::Error>
    where
        K: TryInto<HeaderName>,
        K::Error: Into<http::Error>,
        V: TryInto<HeaderValue>,
        V::Error: Into<http::Error>,
    {
        self.headers.append(
            name.try_into().map_err(Into::into)?,
            value.try_into().map_err(Into::into)?,
        );
        Ok(self)
    }

    /// Append a response trailer.
    ///
    /// Uses [`HeaderMap::append`], so calling twice with the same name
    /// accumulates values rather than replacing.
    ///
    /// # Panics
    ///
    /// Panics if `name` or `value` cannot be converted into the
    /// corresponding header type. Use
    /// [`try_with_trailer`](Self::try_with_trailer) for dynamic
    /// values, or the `trailers` field directly for full control.
    #[must_use]
    pub fn with_trailer<K, V>(mut self, name: K, value: V) -> Self
    where
        K: TryInto<HeaderName>,
        K::Error: std::fmt::Debug,
        V: TryInto<HeaderValue>,
        V::Error: std::fmt::Debug,
    {
        self.trailers
            .append(name.try_into().unwrap(), value.try_into().unwrap());
        self
    }

    /// Append a response trailer, returning an error if `name` or
    /// `value` is invalid.
    ///
    /// Non-panicking sibling of [`with_trailer`](Self::with_trailer)
    /// for dynamic values. Uses [`HeaderMap::append`], so repeated
    /// calls accumulate.
    pub fn try_with_trailer<K, V>(mut self, name: K, value: V) -> Result<Self, http::Error>
    where
        K: TryInto<HeaderName>,
        K::Error: Into<http::Error>,
        V: TryInto<HeaderValue>,
        V::Error: Into<http::Error>,
    {
        self.trailers.append(
            name.try_into().map_err(Into::into)?,
            value.try_into().map_err(Into::into)?,
        );
        Ok(self)
    }

    /// Override the server's compression policy for this response.
    #[must_use]
    pub fn compress(mut self, enabled: bool) -> Self {
        self.compress = Some(enabled);
        self
    }

    /// Replace the body, preserving headers/trailers/compression.
    pub fn map_body<C>(self, f: impl FnOnce(B) -> C) -> Response<C> {
        Response {
            body: f(self.body),
            headers: self.headers,
            trailers: self.trailers,
            compress: self.compress,
        }
    }
}

impl<B> From<B> for Response<B> {
    fn from(body: B) -> Self {
        Self::new(body)
    }
}

impl<T> Response<ServiceStream<T>> {
    /// Wrap a streaming body, boxing and unsize-coercing it to
    /// [`ServiceStream<T>`]. Handles the explicit coercion that
    /// `Ok(Box::pin(s).into())` would otherwise need.
    pub fn stream(s: impl Stream<Item = Result<T, ConnectError>> + Send + 'static) -> Self {
        Self::new(Box::pin(s))
    }
}

/// Result type returned by handler trait methods.
///
/// `B` is the body type — typically the owned response message, or any
/// `impl Encodable<M>`.
pub type ServiceResult<B> = Result<Response<B>, ConnectError>;

/// Boxed `Send` stream of `Result<T, ConnectError>`.
///
/// Used as the request type for client/bidi-streaming handlers and the
/// body type for server/bidi-streaming responses.
pub type ServiceStream<T> = Pin<Box<dyn Stream<Item = Result<T, ConnectError>> + Send>>;

// ---------------------------------------------------------------------------
// Encodable<M>
// ---------------------------------------------------------------------------

/// Encodes to the same wire bytes as proto message `M`.
///
/// This is the bound on the response body in generated trait methods.
/// The blanket impl for `M: Message + Serialize` covers the owned
/// message; a follow-up change adds an impl for buffa's `MView<'_>` so
/// handlers can return borrowed responses.
///
/// # Contract
///
/// Implementations must produce bytes that decode as a valid `M` in
/// the given format.
///
/// `encode` is fallible: the owned-message impl never errors, but a
/// view-body impl is proto-only (view types lack `Serialize`) and must
/// error on `CodecFormat::Json`.
pub trait Encodable<M> {
    /// Encode `self` as wire bytes for `M` in the requested format.
    fn encode(&self, codec: CodecFormat) -> Result<Bytes, ConnectError>;
}

impl<M: Message + Serialize> Encodable<M> for M {
    fn encode(&self, codec: CodecFormat) -> Result<Bytes, ConnectError> {
        match codec {
            CodecFormat::Proto => Ok(self.encode_to_bytes()),
            CodecFormat::Json => serde_json::to_vec(self).map(Bytes::from).map_err(|e| {
                ConnectError::internal(format!("failed to encode JSON response: {e}"))
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// EncodedResponse (dispatcher boundary)
// ---------------------------------------------------------------------------

/// A [`Response`] with the body already encoded to bytes.
///
/// This is what the [`Dispatcher`](crate::Dispatcher) returns to the
/// protocol layer — encoding happens inside the dispatcher so the body
/// type stays generic across the trait boundary.
pub type EncodedResponse = Response<Bytes>;

impl<B> Response<B> {
    /// Encode the body to bytes via [`Encodable<M>`], preserving
    /// response metadata.
    #[doc(hidden)] // exposed for dispatcher::codegen (generated code)
    pub fn encode<M>(self, codec: CodecFormat) -> Result<EncodedResponse, ConnectError>
    where
        B: Encodable<M>,
    {
        let bytes = self.body.encode(codec)?;
        Ok(Response {
            body: bytes,
            headers: self.headers,
            trailers: self.trailers,
            compress: self.compress,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use buffa_types::google::protobuf::StringValue;

    #[test]
    fn response_from_body() {
        let r: Response<StringValue> = StringValue::from("hi").into();
        assert_eq!(r.body.value, "hi");
        assert!(r.headers.is_empty());
        assert!(r.trailers.is_empty());
        assert_eq!(r.compress, None);
    }

    #[test]
    fn response_builder() {
        let r = Response::new(StringValue::from("hi"))
            .with_header("x-a", "1")
            .with_trailer("x-b", "2")
            .compress(true);
        assert_eq!(r.headers.get("x-a").unwrap(), "1");
        assert_eq!(r.trailers.get("x-b").unwrap(), "2");
        assert_eq!(r.compress, Some(true));
    }

    #[test]
    fn encodable_owned_proto() {
        let m = StringValue::from("hello");
        let bytes = Encodable::<StringValue>::encode(&m, CodecFormat::Proto).unwrap();
        assert_eq!(
            StringValue::decode_from_slice(&bytes).unwrap().value,
            "hello"
        );
    }

    #[test]
    fn encodable_owned_json() {
        let m = StringValue::from("hello");
        let bytes = Encodable::<StringValue>::encode(&m, CodecFormat::Json).unwrap();
        assert_eq!(&bytes[..], b"\"hello\"");
    }

    #[test]
    fn response_encode() {
        let r = Response::new(StringValue::from("hi")).with_header("x-a", "1");
        let enc = r.encode::<StringValue>(CodecFormat::Proto).unwrap();
        assert_eq!(enc.headers.get("x-a").unwrap(), "1");
        assert_eq!(
            StringValue::decode_from_slice(&enc.body).unwrap().value,
            "hi"
        );
    }

    #[test]
    fn request_context_new() {
        let mut h = HeaderMap::new();
        h.insert("x-custom", HeaderValue::from_static("v"));
        let ctx = RequestContext::new(h);
        assert_eq!(
            ctx.header(&HeaderName::from_static("x-custom")).unwrap(),
            "v"
        );
        assert!(ctx.deadline.is_none());
    }

    #[test]
    fn request_context_with_deadline() {
        let d = Instant::now();
        let ctx = RequestContext::new(HeaderMap::new()).with_deadline(Some(d));
        assert_eq!(ctx.deadline, Some(d));
    }

    #[test]
    fn response_map_body_preserves_metadata() {
        let r = Response::new(2u32)
            .with_header("x-h", "1")
            .with_trailer("x-t", "2")
            .compress(true);
        let r = r.map_body(|n| n.to_string());
        assert_eq!(r.body, "2");
        assert_eq!(r.headers.get("x-h").unwrap(), "1");
        assert_eq!(r.trailers.get("x-t").unwrap(), "2");
        assert_eq!(r.compress, Some(true));
    }

    #[tokio::test]
    async fn response_stream_yields_items() {
        use futures::StreamExt;
        let r: Response<ServiceStream<i32>> =
            Response::stream(futures::stream::iter([Ok(1), Ok(2), Ok(3)]));
        let collected: Vec<_> = r.body.map(|x| x.unwrap()).collect().await;
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    #[should_panic]
    fn with_header_panics_on_invalid_name() {
        let _ = Response::new(()).with_header("invalid header name", "v");
    }

    #[test]
    fn try_with_header_errors_on_invalid_name() {
        let err = Response::new(())
            .try_with_header("invalid header name", "v")
            .unwrap_err();
        assert!(err.is::<http::header::InvalidHeaderName>());
    }

    #[test]
    fn try_with_header_ok_appends() {
        let r = Response::new(())
            .try_with_header("x-a", "1")
            .unwrap()
            .try_with_header("x-a", "2")
            .unwrap();
        let vals: Vec<_> = r.headers.get_all("x-a").iter().collect();
        assert_eq!(vals.len(), 2);
    }

    #[test]
    fn try_with_trailer_errors_on_invalid_value() {
        // Newlines are not permitted in header values.
        let err = Response::new(())
            .try_with_trailer("x-t", "bad\nvalue")
            .unwrap_err();
        assert!(err.is::<http::header::InvalidHeaderValue>());
    }

    #[test]
    fn request_context_with_extensions() {
        #[derive(Clone, Debug, PartialEq)]
        struct Peer(u32);
        let mut ext = http::Extensions::new();
        ext.insert(Peer(7));
        let ctx = RequestContext::new(HeaderMap::new()).with_extensions(ext);
        assert_eq!(ctx.extensions.get::<Peer>(), Some(&Peer(7)));
    }
}
