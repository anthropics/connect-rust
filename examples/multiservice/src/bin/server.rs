//! Multi-service example ConnectRPC server using axum.
//!
//! Demonstrates registering multiple ConnectRPC services from different
//! protobuf packages into a single axum web server.
//!
//! Run with: `cargo run --bin multiservice-server`
//!
//! Test with:
//!   - `curl http://localhost:8080/health`
//!   - `cargo run --bin multiservice-client`

use std::sync::Arc;
use std::time::SystemTime;

use axum::Router;
use axum::routing::get;
use buffa::view::OwnedView;
use buffa_types::google::protobuf::__buffa::oneof::value;
use buffa_types::google::protobuf::Duration;
use buffa_types::google::protobuf::Struct;
use buffa_types::google::protobuf::Timestamp;
use buffa_types::google::protobuf::Value;
use connectrpc::ConnectError;
use connectrpc::Context;
use connectrpc::Router as ConnectRouter;
use multiservice_example::proto::anthropic::connectrpc::greet::v1::__buffa::view::GreetRequestView;
use multiservice_example::proto::anthropic::connectrpc::math::v1::__buffa::view::AddRequestView;
use multiservice_example::proto::anthropic::connectrpc::wkt::v1::__buffa::view::{
    CalculateDurationRequestView, CreateEventRequestView, ProcessMetadataRequestView,
};
use multiservice_example::*;

/// Implementation of the GreetService trait.
struct MyGreetService;

impl GreetService for MyGreetService {
    async fn greet(
        &self,
        ctx: Context,
        request: OwnedView<GreetRequestView<'static>>,
    ) -> Result<(GreetResponse, Context), ConnectError> {
        let request = request.to_owned_message();
        tracing::info!("Received greet request for: {}", request.name);

        if request.name.is_empty() {
            return Err(ConnectError::invalid_argument("name cannot be empty"));
        }

        let response = GreetResponse {
            message: format!("Hello, {}!", request.name),
            ..Default::default()
        };
        Ok((response, ctx))
    }
}

/// Implementation of the MathService trait.
struct MyMathService;

impl MathService for MyMathService {
    async fn add(
        &self,
        ctx: Context,
        request: OwnedView<AddRequestView<'static>>,
    ) -> Result<(AddResponse, Context), ConnectError> {
        let request = request.to_owned_message();
        tracing::info!("Received add request: {} + {}", request.a, request.b);

        let result = request
            .a
            .checked_add(request.b)
            .ok_or_else(|| ConnectError::invalid_argument("arithmetic overflow"))?;

        let response = AddResponse {
            result,
            ..Default::default()
        };
        Ok((response, ctx))
    }
}

/// Implementation of the WellKnownTypesService trait.
/// Demonstrates usage of Timestamp, Duration, and Struct types.
struct MyWellKnownTypesService;

impl WellKnownTypesService for MyWellKnownTypesService {
    async fn create_event(
        &self,
        ctx: Context,
        request: OwnedView<CreateEventRequestView<'static>>,
    ) -> Result<(CreateEventResponse, Context), ConnectError> {
        let request = request.to_owned_message();
        tracing::info!("Received create_event request: {:?}", request.name);

        let now = SystemTime::now();
        let now_duration = now.duration_since(std::time::UNIX_EPOCH).unwrap();
        let now_timestamp = Timestamp {
            seconds: now_duration.as_secs() as i64,
            nanos: now_duration.subsec_nanos() as i32,
            ..Default::default()
        };
        let occurred_at = if request.occurred_at.is_set() {
            (*request.occurred_at).clone()
        } else {
            now_timestamp.clone()
        };
        let created_at = now_timestamp;

        let duration = if request.duration.is_set() {
            (*request.duration).clone()
        } else {
            Duration {
                seconds: 3600,
                nanos: 0,
                ..Default::default()
            }
        };

        let id = format!("evt_{}", now_duration.as_millis());

        let event = Event {
            id,
            name: request.name,
            occurred_at: occurred_at.into(),
            duration: duration.into(),
            created_at: created_at.into(),
            ..Default::default()
        };

        let response = CreateEventResponse {
            event: event.into(),
            ..Default::default()
        };
        Ok((response, ctx))
    }

    async fn calculate_duration(
        &self,
        ctx: Context,
        request: OwnedView<CalculateDurationRequestView<'static>>,
    ) -> Result<(CalculateDurationResponse, Context), ConnectError> {
        let request = request.to_owned_message();
        tracing::info!("Received calculate_duration request");

        let start = request
            .start
            .as_option()
            .ok_or_else(|| ConnectError::invalid_argument("start timestamp is required"))?;
        let end = request
            .end
            .as_option()
            .ok_or_else(|| ConnectError::invalid_argument("end timestamp is required"))?;

        let start_nanos = start.seconds * 1_000_000_000 + start.nanos as i64;
        let end_nanos = end.seconds * 1_000_000_000 + end.nanos as i64;
        let diff_nanos = end_nanos - start_nanos;

        let duration = Duration {
            seconds: diff_nanos / 1_000_000_000,
            nanos: (diff_nanos % 1_000_000_000) as i32,
            ..Default::default()
        };

        let response = CalculateDurationResponse {
            duration: duration.into(),
            ..Default::default()
        };
        Ok((response, ctx))
    }

    async fn process_metadata(
        &self,
        ctx: Context,
        request: OwnedView<ProcessMetadataRequestView<'static>>,
    ) -> Result<(ProcessMetadataResponse, Context), ConnectError> {
        let request = request.to_owned_message();
        tracing::info!("Received process_metadata request");

        let input_metadata = if request.metadata.is_set() {
            (*request.metadata).clone()
        } else {
            Struct::default()
        };
        let field_count = input_metadata.fields.len() as i32;

        let mut output_fields = input_metadata.fields.clone();
        output_fields.insert(
            "processed".to_string(),
            Value {
                kind: Some(value::Kind::BoolValue(true)),
                ..Default::default()
            },
        );
        output_fields.insert(
            "original_field_count".to_string(),
            Value {
                kind: Some(value::Kind::NumberValue(field_count as f64)),
                ..Default::default()
            },
        );

        let response = ProcessMetadataResponse {
            metadata: Struct {
                fields: output_fields,
                ..Default::default()
            }
            .into(),
            field_count,
            ..Default::default()
        };
        Ok((response, ctx))
    }
}

async fn health() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let greet_service = Arc::new(MyGreetService);
    let math_service = Arc::new(MyMathService);
    let well_known_types_service = Arc::new(MyWellKnownTypesService);

    let connect_router = greet_service.register(ConnectRouter::new());
    let connect_router = math_service.register(connect_router);
    let connect_router = well_known_types_service.register(connect_router);

    tracing::info!("Registered RPC methods:");
    for method in connect_router.methods() {
        tracing::info!("  POST /{method}");
    }

    let app = Router::new()
        .route("/health", get(health))
        .fallback_service(connect_router.into_axum_service());

    let addr = std::env::var("ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    tracing::info!("Starting server on http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
