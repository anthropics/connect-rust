//! gRPC health-checking service for `connectrpc`.
//!
//! Wire-compatible with [`grpc.health.v1.Health`], so `grpc_health_probe`,
//! `grpcurl`, Kubernetes' gRPC liveness probes, and any other client of the
//! standard gRPC health protocol just work.
//!
//! [`grpc.health.v1.Health`]: https://github.com/grpc/grpc-proto/blob/master/grpc/health/v1/health.proto

mod checker;
mod status;

#[allow(clippy::upper_case_acronyms)]
#[path = "generated/connect/mod.rs"]
mod connect;
#[allow(clippy::upper_case_acronyms)]
#[path = "generated/buffa/mod.rs"]
mod proto;

pub use checker::{Checker, StatusStream};
pub use status::Status;

/// Generated client for calling a `grpc.health.v1.Health` server.
pub use connect::grpc::health::v1::HealthClient;

/// Generated extension trait that adds `.register(router)` to any
/// `Arc<S> where S: Health`.
pub use connect::grpc::health::v1::HealthExt;

/// Fully-qualified protobuf service name: `"grpc.health.v1.Health"`.
pub use connect::grpc::health::v1::HEALTH_SERVICE_NAME;

/// Re-exports of the generated `grpc.health.v1` wire types — request and
/// response messages, `ServingStatus`, the `*_SPEC` constants. Downstream
/// crates can build probe loops without regenerating the proto.
pub mod wire {
    pub use crate::connect::grpc::health::v1::{HEALTH_CHECK_SPEC, HEALTH_WATCH_SPEC};
    pub use crate::proto::grpc::health::v1::health_check_response::ServingStatus;
    pub use crate::proto::grpc::health::v1::{HealthCheckRequest, HealthCheckResponse};
}
