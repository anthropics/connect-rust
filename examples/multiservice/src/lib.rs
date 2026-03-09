//! ConnectRPC Multi-Service Example
//!
//! This crate demonstrates registering multiple ConnectRPC services from
//! different protobuf packages into a single server.

pub mod generated;

// Re-export from greet subpackage
pub use generated::anthropic::connectrpc::greet::v1::{
    GREET_SERVICE_SERVICE_NAME, GreetRequest, GreetResponse, GreetService, GreetServiceClient,
    GreetServiceExt,
};

// Re-export from math subpackage
pub use generated::anthropic::connectrpc::math::v1::{
    AddRequest, AddResponse, MATH_SERVICE_SERVICE_NAME, MathService, MathServiceClient,
    MathServiceExt,
};

// Re-export from wkt subpackage
pub use generated::anthropic::connectrpc::wkt::v1::{
    CalculateDurationRequest, CalculateDurationResponse, CreateEventRequest, CreateEventResponse,
    Event, ProcessMetadataRequest, ProcessMetadataResponse, WELL_KNOWN_TYPES_SERVICE_SERVICE_NAME,
    WellKnownTypesService, WellKnownTypesServiceClient, WellKnownTypesServiceExt,
};
