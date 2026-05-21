//! The `Status` enum returned by checkers and streamed by watchers.
//!
//! `Status` is the Rust-side type for [`Checker`](crate::Checker) impls and
//! [`StaticChecker`](crate::StaticChecker); the server maps it to
//! [`wire::ServingStatus`](crate::wire::ServingStatus) automatically. Reach
//! for the wire enum only when decoding a raw `HealthCheckResponse` off the
//! network (e.g. in a probe loop). `SERVICE_UNKNOWN` is intentionally not
//! represented. Unknown services surface as `NotFound` instead.

use crate::proto::grpc::health::v1::health_check_response::ServingStatus;

/// Health status of a single service or of the whole server.
///
/// `Status::default()` is [`Status::Unknown`] (the proto wire default), not
/// [`Status::Serving`] — [`StaticChecker::with_services`] seeds new entries
/// with `Serving` because that's almost always what registering a service
/// means in practice.
///
/// [`StaticChecker::with_services`]: crate::StaticChecker::with_services
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Status {
    /// The implementation has not (yet) determined whether the service
    /// is healthy.
    #[default]
    Unknown,
    /// The service is ready to accept requests.
    Serving,
    /// The process is up but the service is intentionally not accepting
    /// requests (e.g. a dependency is down, or the service is draining
    /// in preparation for shutdown).
    NotServing,
}

impl Status {
    /// Lowercase string representation, useful for logging.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Serving => "serving",
            Self::NotServing => "not_serving",
        }
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<Status> for ServingStatus {
    fn from(value: Status) -> Self {
        match value {
            Status::Unknown => ServingStatus::UNKNOWN,
            Status::Serving => ServingStatus::SERVING,
            Status::NotServing => ServingStatus::NOT_SERVING,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_unknown() {
        assert_eq!(Status::default(), Status::Unknown);
    }

    #[test]
    fn as_str_lowercase() {
        assert_eq!(Status::Unknown.as_str(), "unknown");
        assert_eq!(Status::Serving.as_str(), "serving");
        assert_eq!(Status::NotServing.as_str(), "not_serving");
    }

    #[test]
    fn maps_to_serving_status() {
        assert_eq!(ServingStatus::from(Status::Unknown), ServingStatus::UNKNOWN);
        assert_eq!(ServingStatus::from(Status::Serving), ServingStatus::SERVING);
        assert_eq!(
            ServingStatus::from(Status::NotServing),
            ServingStatus::NOT_SERVING,
        );
    }
}
