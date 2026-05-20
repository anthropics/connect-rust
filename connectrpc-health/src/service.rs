//! The bridge from a user [`Checker`] to the generated `grpc.health.v1.Health`
//! service trait.

use std::sync::Arc;

use buffa::view::OwnedView;
use connectrpc::{ConnectError, RequestContext, Response, ServiceResult, ServiceStream};
use futures::StreamExt;

use crate::Checker;
use crate::connect::grpc::health::v1::Health;
use crate::proto::grpc::health::v1::{
    HealthCheckRequestView, HealthCheckResponse, health_check_response::ServingStatus,
};

/// gRPC-compatible health service backed by a user-supplied [`Checker`].
///
/// Wraps any `Checker` and exposes it as the wire-format
/// `grpc.health.v1.Health` service. Register it like any other generated
/// service — wrap in an `Arc` and call `.register(router)`:
///
/// ```no_run
/// use std::sync::Arc;
/// use connectrpc::Router;
/// use connectrpc_health::{HealthExt, HealthService, StaticChecker};
///
/// let checker = Arc::new(StaticChecker::with_services([
///     "acme.user.v1.UserService",
/// ]));
/// let service = Arc::new(HealthService::from_arc(Arc::clone(&checker)));
/// let router = service.register(Router::new());
/// ```
///
/// `HealthService::new(checker)` is the move-in shorthand; use
/// [`from_arc`](Self::from_arc) when you keep your own clone of the
/// `Arc<C>` to flip status from outside the service.
///
/// # Unknown services
///
/// Non-empty unregistered services surface as
/// `Err(ConnectError::not_found(_))` from both `Check` and `Watch`; the
/// empty service is pre-registered with [`Status::Serving`] and behaves
/// like any other service (see [`StaticChecker`]'s `# Empty service name`
/// section). The [gRPC Health spec] additionally specifies a
/// `SERVICE_UNKNOWN` keep-stream-open flow for `Watch` that this crate
/// does not implement (matching the Go `connectrpc.com/grpchealth`
/// reference). Probes that treat any error as failure — kubelet,
/// `grpc_health_probe`, Linkerd, Istio — work unchanged.
///
/// [`Status::Serving`]: crate::Status::Serving
/// [`StaticChecker`]: crate::StaticChecker
///
/// [gRPC Health spec]: https://github.com/grpc/grpc/blob/master/doc/health-checking.md
pub struct HealthService<C> {
    checker: Arc<C>,
}

impl<C: Checker> HealthService<C> {
    /// Wrap a checker by value; it is moved into a fresh `Arc<C>`.
    #[must_use]
    pub fn new(checker: C) -> Self {
        Self {
            checker: Arc::new(checker),
        }
    }

    /// Wrap a checker that is already inside an `Arc<C>`. Use this when
    /// you keep your own clone of the `Arc<C>` to flip status from
    /// outside the service.
    #[must_use]
    pub fn from_arc(checker: Arc<C>) -> Self {
        Self { checker }
    }

    /// Borrow the inner `Arc<C>`. Clone it if you need a long-lived
    /// handle for mutation.
    #[must_use]
    pub fn checker(&self) -> &Arc<C> {
        &self.checker
    }
}

impl<C> Clone for HealthService<C> {
    fn clone(&self) -> Self {
        Self {
            checker: Arc::clone(&self.checker),
        }
    }
}

impl<C> std::fmt::Debug for HealthService<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HealthService").finish_non_exhaustive()
    }
}

impl<C: Checker> Health for HealthService<C> {
    async fn check(
        &self,
        _ctx: RequestContext,
        request: OwnedView<HealthCheckRequestView<'static>>,
    ) -> ServiceResult<HealthCheckResponse> {
        let status = self.checker.check(request.service).await?;
        Response::ok(HealthCheckResponse {
            status: ServingStatus::from(status).into(),
            ..Default::default()
        })
    }

    async fn watch(
        &self,
        _ctx: RequestContext,
        request: OwnedView<HealthCheckRequestView<'static>>,
    ) -> ServiceResult<ServiceStream<HealthCheckResponse>> {
        let stream = self.checker.watch(request.service).await?;
        let stream = stream.map(|status| {
            Ok::<_, ConnectError>(HealthCheckResponse {
                status: ServingStatus::from(status).into(),
                ..Default::default()
            })
        });
        Response::stream_ok(stream.boxed())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use connectrpc::Router;
    use connectrpc::client::{ClientConfig, HttpClient};
    use tokio::net::TcpListener;

    use super::*;
    use crate::connect::grpc::health::v1::{HealthClient, HealthExt};
    use crate::proto::grpc::health::v1::HealthCheckRequest;
    use crate::{StaticChecker, Status};

    /// Spin up a Health server on a free port and hand back the address
    /// and a client targeting it. The server runs until the test exits.
    async fn spawn_health_server(
        checker: Arc<StaticChecker>,
    ) -> (HealthClient<HttpClient>, std::net::SocketAddr) {
        let service = Arc::new(HealthService::from_arc(checker));
        let router = service.register(Router::new());
        let app = router.into_axum_router();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let config = ClientConfig::new(format!("http://{addr}").parse().unwrap());
        let client = HealthClient::new(HttpClient::plaintext(), config);
        (client, addr)
    }

    #[tokio::test]
    async fn check_serving_service() {
        let checker = Arc::new(StaticChecker::with_services(["acme.A"]));
        let (client, _addr) = spawn_health_server(checker).await;

        let resp = client
            .check(HealthCheckRequest {
                service: "acme.A".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(resp.view().status, ServingStatus::SERVING);
    }

    #[tokio::test]
    async fn check_empty_service_returns_serving() {
        let checker = Arc::new(StaticChecker::new());
        let (client, _addr) = spawn_health_server(checker).await;

        let resp = client.check(HealthCheckRequest::default()).await.unwrap();
        assert_eq!(resp.view().status, ServingStatus::SERVING);
    }

    #[tokio::test]
    async fn check_unknown_service_returns_not_found() {
        let checker = Arc::new(StaticChecker::new());
        let (client, _addr) = spawn_health_server(checker).await;

        let err = client
            .check(HealthCheckRequest {
                service: "acme.NoSuch".into(),
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert_eq!(err.code, connectrpc::ErrorCode::NotFound);
    }

    #[tokio::test]
    async fn check_reflects_not_serving_after_update() {
        let checker = Arc::new(StaticChecker::with_services(["acme.A"]));
        let (client, _addr) = spawn_health_server(Arc::clone(&checker)).await;

        checker.set_status("acme.A", Status::NotServing);

        let resp = client
            .check(HealthCheckRequest {
                service: "acme.A".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(resp.view().status, ServingStatus::NOT_SERVING);
    }

    #[tokio::test]
    async fn watch_streams_initial_then_changes() {
        let checker = Arc::new(StaticChecker::with_services(["acme.A"]));
        let (client, _addr) = spawn_health_server(Arc::clone(&checker)).await;

        let mut stream = client
            .watch(HealthCheckRequest {
                service: "acme.A".into(),
                ..Default::default()
            })
            .await
            .unwrap();

        // First message is the current state.
        let initial = stream
            .message()
            .await
            .unwrap()
            .expect("expected initial Watch message");
        assert_eq!(initial.status, ServingStatus::SERVING);

        // Update fires a follow-up message.
        checker.set_status("acme.A", Status::NotServing);
        let after = tokio::time::timeout(Duration::from_secs(2), stream.message())
            .await
            .expect("watch did not deliver update within timeout")
            .unwrap()
            .expect("expected follow-up Watch message");
        assert_eq!(after.status, ServingStatus::NOT_SERVING);
    }

    #[tokio::test]
    async fn checker_accessor_returns_shared_arc() {
        let svc = HealthService::new(StaticChecker::with_services(["acme.A"]));
        // Mutating through the accessor must be visible to the service.
        svc.checker().set_status("acme.A", Status::NotServing);
        let (client, _addr) = spawn_health_server(Arc::clone(svc.checker())).await;
        let resp = client
            .check(HealthCheckRequest {
                service: "acme.A".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(resp.view().status, ServingStatus::NOT_SERVING);
    }

    #[tokio::test]
    async fn watch_unimplemented_when_checker_does_not_support_it() {
        struct CheckOnly;
        impl Checker for CheckOnly {
            async fn check(&self, _service: &str) -> Result<Status, ConnectError> {
                Ok(Status::Serving)
            }
            // No watch override → default returns Unimplemented.
        }
        let svc = Arc::new(HealthService::new(CheckOnly));
        let router = svc.register(Router::new());
        let app = router.into_axum_router();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let config = ClientConfig::new(format!("http://{addr}").parse().unwrap());
        let client: HealthClient<HttpClient> = HealthClient::new(HttpClient::plaintext(), config);

        let mut stream = client.watch(HealthCheckRequest::default()).await.unwrap();
        // Server-streaming RPCs surface errors via the trailers — `message()`
        // returns `Ok(None)` and the error lands on `stream.error()`.
        assert!(stream.message().await.unwrap().is_none());
        let err = stream.error().expect("expected Unimplemented error");
        assert_eq!(err.code, connectrpc::ErrorCode::Unimplemented);
    }
}
