//! gRPC health-checking service for `connectrpc`.
//!
//! Wire-compatible with [`grpc.health.v1.Health`], so `grpc_health_probe`,
//! `grpcurl`, Kubernetes' gRPC liveness probes, and any other client of the
//! standard gRPC health protocol just work.
//!
//! Non-empty unregistered services return `Err(ConnectError::not_found(_))`
//! from both `Check` and `Watch`; the empty service auto-subscribes on
//! `Watch` and returns `Serving` on `Check` by default — see
//! [`HealthService`]'s `# Unknown services` section for how this relates
//! to the gRPC Health spec.
//!
//! # Quick start
//!
//! ```no_run
//! use std::sync::Arc;
//! use connectrpc::Router;
//! use connectrpc_health::{HealthExt, HealthService, StaticChecker, Status};
//!
//! // In real code, pass the generated `*_SERVICE_NAME` constant —
//! // the literal below is a stand-in.
//! let checker = Arc::new(StaticChecker::with_services([
//!     "acme.user.v1.UserService",
//! ]));
//!
//! let service = Arc::new(HealthService::from_arc(Arc::clone(&checker)));
//! let router = service.register(Router::new());
//!
//! // Later, when something goes wrong:
//! checker.set_status("acme.user.v1.UserService", Status::NotServing);
//!
//! // ...and at shutdown. `shutdown()` flips every registered service,
//! // including the empty whole-process entry seeded on construction.
//! checker.shutdown();
//! ```
//!
//! [`grpc.health.v1.Health`]: https://github.com/grpc/grpc-proto/blob/master/grpc/health/v1/health.proto

mod checker;
mod service;
mod static_checker;
mod status;

#[allow(clippy::upper_case_acronyms)]
#[path = "generated/connect/mod.rs"]
mod connect;
#[allow(clippy::upper_case_acronyms)]
#[path = "generated/buffa/mod.rs"]
mod proto;

pub use checker::{Checker, StatusStream};
pub use service::HealthService;
pub use static_checker::StaticChecker;
pub use status::Status;

/// Generated client for calling a `grpc.health.v1.Health` server.
pub use connect::grpc::health::v1::HealthClient;

/// Generated extension trait that adds `.register(router)` to any
/// `Arc<S> where S: Health`. Import it to register a [`HealthService`].
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
