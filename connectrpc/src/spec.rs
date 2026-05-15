//! Static RPC method metadata.
//!
//! [`Spec`] describes a single RPC procedure independent of any particular
//! request: its fully-qualified path, stream type, idempotency level, and
//! whether the artifact carrying the spec sits on the client or server side
//! of the wire. Code generation emits one `Spec` constant per method; the
//! runtime threads it through to handlers and (in a later release) to RPC
//! interceptors so they can label spans, route, and gate behaviour without
//! re-parsing the request URL.
//!
//! `Spec` deliberately carries only **registration-time** facts. Per-request
//! state — negotiated protocol, codec, deadline — lives on
//! [`RequestContext`](crate::RequestContext). This mirrors the split in
//! `connect-go`, where `Spec` describes the method and `Peer` describes the
//! connection.

use crate::router::MethodKind;

/// The shape of an RPC: how many messages flow in each direction.
///
/// This is the interceptor-facing equivalent of [`MethodKind`] and uses the
/// `connect-go` naming so cross-runtime interceptor logic ports cleanly.
/// Convert with [`From`] in either direction.
///
/// `StreamType` is intentionally exhaustive — the four shapes are fixed by
/// the gRPC and Connect protocols. [`MethodKind`] is the routing-table
/// equivalent used by [`Router`](crate::Router) registration; prefer
/// `StreamType` in code that consumes a [`Spec`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StreamType {
    /// One request message, one response message.
    Unary,
    /// A stream of request messages, one response message.
    ClientStream,
    /// One request message, a stream of response messages.
    ServerStream,
    /// Streams of request and response messages.
    BidiStream,
}

impl From<MethodKind> for StreamType {
    fn from(kind: MethodKind) -> Self {
        match kind {
            MethodKind::Unary => Self::Unary,
            MethodKind::ClientStreaming => Self::ClientStream,
            MethodKind::ServerStreaming => Self::ServerStream,
            MethodKind::BidiStreaming => Self::BidiStream,
        }
    }
}

impl From<StreamType> for MethodKind {
    fn from(st: StreamType) -> Self {
        match st {
            StreamType::Unary => Self::Unary,
            StreamType::ClientStream => Self::ClientStreaming,
            StreamType::ServerStream => Self::ServerStreaming,
            StreamType::BidiStream => Self::BidiStreaming,
        }
    }
}

/// The idempotency contract a method declares via
/// `option idempotency_level` in its proto definition.
///
/// Connect uses this to decide whether a unary call may be retried or sent
/// over an HTTP `GET` request. Interceptors can use it to make the same
/// decision — for example, a retry interceptor should only retry calls that
/// declare [`NoSideEffects`](IdempotencyLevel::NoSideEffects) or
/// [`Idempotent`](IdempotencyLevel::Idempotent).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum IdempotencyLevel {
    /// The method makes no idempotency guarantee. This is the proto default.
    #[default]
    Unknown,
    /// The method is read-only and safe to retry or send via `GET`.
    NoSideEffects,
    /// The method may have side effects, but repeating it with the same
    /// request is safe.
    Idempotent,
}

/// Static description of an RPC method.
///
/// One `Spec` value exists per generated method, emitted as a
/// `pub const … : Spec` in the generated service module and surfaced on
/// [`RequestContext::spec`](crate::RequestContext::spec) for handlers. It
/// names the method (`/package.Service/Method`), its stream shape, and its
/// proto-declared idempotency contract.
///
/// `Spec` is `Copy` and contains only `'static` data, so it can be stored,
/// captured in closures, and compared freely with no allocation.
///
/// Construct one with [`Spec::new`]. The struct is `#[non_exhaustive]` so
/// future fields can be added without a breaking change; destructure with a
/// trailing `..` (e.g. `let Spec { procedure, stream_type, .. } = spec`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Spec {
    /// The fully-qualified procedure path, `"/package.Service/Method"`.
    ///
    /// Includes the leading slash to match the HTTP request URI and the
    /// OpenTelemetry `rpc.method` convention. The runtime strips the leading
    /// slash before [`Dispatcher::lookup`](crate::Dispatcher::lookup); use
    /// `procedure.trim_start_matches('/')` to compare against routing keys.
    pub procedure: &'static str,
    /// The message-flow shape of the method.
    pub stream_type: StreamType,
    /// `true` when this `Spec` was produced by a generated client, `false`
    /// for a server-side dispatcher.
    ///
    /// Generated server-side `*_SPEC` constants always carry `false`. Once
    /// the client-side interceptor surface lands, the generated client will
    /// supply specs with `is_client: true` so a single interceptor
    /// registered on both sides can distinguish.
    pub is_client: bool,
    /// The idempotency contract declared in the proto definition.
    ///
    /// This is the full three-valued proto enum. The boolean
    /// [`MethodDescriptor::idempotent`](crate::dispatcher::MethodDescriptor::idempotent)
    /// is a *derived* "Connect GET-eligible" flag that is only `true` for
    /// [`NoSideEffects`](IdempotencyLevel::NoSideEffects) — `Idempotent`
    /// methods are safe to retry but not GET-eligible.
    pub idempotency: IdempotencyLevel,
}

impl Spec {
    /// Construct a `Spec` with default `is_client` (`false`) and
    /// `idempotency` ([`IdempotencyLevel::Unknown`]).
    ///
    /// Generated code chains [`with_idempotency`](Spec::with_idempotency)
    /// onto this constructor in `const` position, so `Spec` constants live
    /// in `.rodata`.
    pub const fn new(procedure: &'static str, stream_type: StreamType) -> Self {
        Self {
            procedure,
            stream_type,
            is_client: false,
            idempotency: IdempotencyLevel::Unknown,
        }
    }

    /// Set the idempotency level. Returns `self` for chaining in `const`
    /// position.
    #[must_use]
    pub const fn with_idempotency(mut self, idempotency: IdempotencyLevel) -> Self {
        self.idempotency = idempotency;
        self
    }

    /// The bare service name (`"package.Service"`) from
    /// [`procedure`](Spec::procedure), without the leading slash or trailing
    /// `/Method`.
    ///
    /// Returns the whole procedure (sans leading `/`) if it contains no
    /// method separator, which never happens for generated specs.
    // TODO: make `const` once `str::rsplit_once` is const-stable.
    pub fn service(&self) -> &'static str {
        let p = self.procedure.trim_start_matches('/');
        p.rsplit_once('/').map(|(svc, _)| svc).unwrap_or(p)
    }

    /// The bare method name (`"Method"`) from [`procedure`](Spec::procedure).
    ///
    /// Returns the whole procedure (sans leading `/`) if it contains no
    /// method separator, which never happens for generated specs.
    // TODO: make `const` once `str::rsplit_once` is const-stable.
    pub fn method(&self) -> &'static str {
        let p = self.procedure.trim_start_matches('/');
        p.rsplit_once('/').map(|(_, m)| m).unwrap_or(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_type_round_trips_method_kind() {
        for kind in [
            MethodKind::Unary,
            MethodKind::ServerStreaming,
            MethodKind::ClientStreaming,
            MethodKind::BidiStreaming,
        ] {
            assert_eq!(MethodKind::from(StreamType::from(kind)), kind);
        }
    }

    #[test]
    fn spec_const_construction_and_accessors() {
        const SPEC: Spec = Spec::new("/pkg.Greet/Say", StreamType::Unary)
            .with_idempotency(IdempotencyLevel::NoSideEffects);
        assert_eq!(SPEC.procedure, "/pkg.Greet/Say");
        assert_eq!(SPEC.service(), "pkg.Greet");
        assert_eq!(SPEC.method(), "Say");
        assert_eq!(SPEC.stream_type, StreamType::Unary);
        assert_eq!(SPEC.idempotency, IdempotencyLevel::NoSideEffects);
        const { assert!(!SPEC.is_client) };
    }

    #[test]
    fn spec_defaults() {
        let s = Spec::new("/a.B/C", StreamType::BidiStream);
        assert_eq!(s.idempotency, IdempotencyLevel::Unknown);
        assert!(!s.is_client);
    }

    #[test]
    fn spec_service_method_no_separator() {
        // Degenerate case: no '/Method' component. Both accessors fall back
        // to the whole (de-slashed) string rather than panicking.
        let s = Spec::new("nopath", StreamType::Unary);
        assert_eq!(s.service(), "nopath");
        assert_eq!(s.method(), "nopath");
    }
}
