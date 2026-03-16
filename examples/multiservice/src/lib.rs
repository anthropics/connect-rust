//! ConnectRPC Multi-Service Example
//!
//! This crate demonstrates registering multiple ConnectRPC services from
//! different protobuf packages into a single server.

#[path = "generated/connect/mod.rs"]
pub mod connect;
#[path = "generated/buffa/mod.rs"]
pub mod proto;

// Re-export from greet subpackage
pub use connect::anthropic::connectrpc::greet::v1::{
    GREET_SERVICE_SERVICE_NAME, GreetService, GreetServiceClient, GreetServiceExt,
};
pub use proto::anthropic::connectrpc::greet::v1::{GreetRequest, GreetResponse};

// Re-export from math subpackage
pub use connect::anthropic::connectrpc::math::v1::{
    MATH_SERVICE_SERVICE_NAME, MathService, MathServiceClient, MathServiceExt,
};
pub use proto::anthropic::connectrpc::math::v1::{AddRequest, AddResponse};

// Re-export from wkt subpackage
pub use connect::anthropic::connectrpc::wkt::v1::{
    WELL_KNOWN_TYPES_SERVICE_SERVICE_NAME, WellKnownTypesService, WellKnownTypesServiceClient,
    WellKnownTypesServiceExt,
};
pub use proto::anthropic::connectrpc::wkt::v1::{
    CalculateDurationRequest, CalculateDurationResponse, CreateEventRequest, CreateEventResponse,
    Event, ProcessMetadataRequest, ProcessMetadataResponse,
};
