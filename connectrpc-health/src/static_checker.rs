//! A shared-state [`Checker`] suitable for most servers.

use std::collections::HashMap;
use std::sync::Mutex;

use connectrpc::ConnectError;
use tokio::sync::watch;

use crate::checker::StatusStream;
use crate::{Checker, Status};

/// In-memory checker backed by a `HashMap<String, Status>`.
///
/// `Send + Sync`; clone the `Arc` you wrap it in to share across tasks.
///
/// # Empty service name
///
/// The empty string represents the whole-process status. It is always
/// pre-registered with [`Status::Serving`], so `check("")` and
/// `watch("")` behave like any other registered service — and
/// [`shutdown`](Self::shutdown) flips it alongside the user-registered
/// services. Unregistered non-empty services return `NotFound` from
/// both `check` and `watch`.
pub struct StaticChecker {
    services: Mutex<HashMap<String, watch::Sender<Status>>>,
}

impl StaticChecker {
    /// Create a checker with only the whole-process entry (`""`) seeded
    /// at [`Status::Serving`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_services(std::iter::empty::<&str>())
    }

    /// Create a checker pre-populated with the given services and the
    /// whole-process entry (`""`), each reporting [`Status::Serving`].
    /// Pass the generated `*_SERVICE_NAME` constant to avoid typos.
    #[must_use]
    pub fn with_services<I, S>(services: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let map = services
            .into_iter()
            .map(|s| (s.into(), watch::channel(Status::Serving).0))
            .chain(std::iter::once((
                String::new(),
                watch::channel(Status::Serving).0,
            )))
            .collect();
        Self {
            services: Mutex::new(map),
        }
    }

    /// Update the status of `service`, registering it if it was not
    /// previously known. Existing `Watch` subscribers are notified;
    /// no-op transitions are suppressed.
    pub fn set_status(&self, service: impl Into<String>, status: Status) {
        let mut services = self.lock();
        services
            .entry(service.into())
            .and_modify(|sender| {
                sender.send_if_modified(|current| {
                    if *current == status {
                        false
                    } else {
                        *current = status;
                        true
                    }
                });
            })
            .or_insert_with(|| watch::channel(status).0);
    }

    /// Mark every registered service [`Status::NotServing`], including
    /// the whole-process `""` entry. Call this from your shutdown handler
    /// before draining traffic. Services registered after `shutdown` are
    /// unaffected.
    pub fn shutdown(&self) {
        let services = self.lock();
        for sender in services.values() {
            sender.send_if_modified(|status| {
                if *status == Status::NotServing {
                    false
                } else {
                    *status = Status::NotServing;
                    true
                }
            });
        }
    }

    /// Snapshot of every registered service name. Always includes the
    /// whole-process entry `""`, plus any names supplied via
    /// [`with_services`](Self::with_services) or [`set_status`](Self::set_status).
    #[must_use]
    pub fn services(&self) -> Vec<String> {
        self.lock().keys().cloned().collect()
    }

    // Poison recovery: the wrapped state is `Status` + `watch::Sender`,
    // both safe to observe after a panic, so we keep going instead of
    // turning the next handler call into a second panic.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, watch::Sender<Status>>> {
        self.services
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Default for StaticChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl Checker for StaticChecker {
    async fn check(&self, service: &str) -> Result<Status, ConnectError> {
        // Block-scope the guard: forces `Send`-incompatible state to drop
        // before any future code path could grow an `.await`.
        let snapshot = {
            let services = self.lock();
            services.get(service).map(|sender| *sender.borrow())
        };
        snapshot.ok_or_else(|| ConnectError::not_found(format!("unknown service {service}")))
    }

    async fn watch(&self, service: &str) -> Result<StatusStream, ConnectError> {
        let receiver = {
            let services = self.lock();
            services.get(service).map(watch::Sender::subscribe)
        };
        receiver
            .map(StatusStream::from_watch)
            .ok_or_else(|| ConnectError::not_found(format!("unknown service {service}")))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures::StreamExt;

    use super::*;

    #[tokio::test]
    async fn check_unknown_service_returns_not_found() {
        let checker = StaticChecker::new();
        let err = checker.check("acme.NoSuch").await.unwrap_err();
        assert_eq!(err.code, connectrpc::ErrorCode::NotFound);
    }

    #[tokio::test]
    async fn check_empty_service_defaults_to_serving() {
        let checker = StaticChecker::new();
        assert_eq!(checker.check("").await.unwrap(), Status::Serving);
    }

    #[tokio::test]
    async fn with_services_seeds_serving() {
        let checker = StaticChecker::with_services(["acme.A", "acme.B"]);
        assert_eq!(checker.check("acme.A").await.unwrap(), Status::Serving);
        assert_eq!(checker.check("acme.B").await.unwrap(), Status::Serving);
    }

    #[tokio::test]
    async fn set_status_registers_new_service() {
        let checker = StaticChecker::new();
        checker.set_status("acme.A", Status::NotServing);
        assert_eq!(checker.check("acme.A").await.unwrap(), Status::NotServing);
    }

    #[tokio::test]
    async fn shutdown_marks_all_not_serving() {
        let checker = StaticChecker::with_services(["acme.A", "acme.B"]);
        checker.shutdown();
        assert_eq!(checker.check("acme.A").await.unwrap(), Status::NotServing);
        assert_eq!(checker.check("acme.B").await.unwrap(), Status::NotServing);
    }

    #[tokio::test]
    async fn shutdown_leaves_post_registered_services_alone() {
        let checker = StaticChecker::with_services(["acme.A"]);
        checker.shutdown();
        // Registering after shutdown is the documented escape hatch — the
        // new service must come up Serving, not NotServing.
        checker.set_status("acme.B", Status::Serving);
        assert_eq!(checker.check("acme.B").await.unwrap(), Status::Serving);
    }

    #[tokio::test]
    async fn shutdown_is_noop_for_already_not_serving() {
        let checker = StaticChecker::with_services(["acme.A"]);
        checker.set_status("acme.A", Status::NotServing);
        let mut stream = checker.watch("acme.A").await.unwrap();
        assert_eq!(stream.next().await.unwrap(), Status::NotServing);

        // Already NotServing → shutdown must not emit a notification.
        checker.shutdown();
        tokio::select! {
            item = stream.next() => panic!("unexpected notification on no-op shutdown: {item:?}"),
            () = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
        }
    }

    #[tokio::test]
    async fn watch_streams_initial_and_changes() {
        let checker = StaticChecker::with_services(["acme.A"]);
        let mut stream = checker.watch("acme.A").await.unwrap();

        // Initial value is the current state.
        assert_eq!(stream.next().await.unwrap(), Status::Serving);

        // Update fires the subscriber.
        checker.set_status("acme.A", Status::NotServing);
        let next = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
            .await
            .expect("watch did not deliver update within timeout")
            .unwrap();
        assert_eq!(next, Status::NotServing);
    }

    #[tokio::test]
    async fn watch_unknown_service_returns_not_found() {
        let checker = StaticChecker::new();
        let err = checker.watch("acme.NoSuch").await.unwrap_err();
        assert_eq!(err.code, connectrpc::ErrorCode::NotFound);
    }

    #[tokio::test]
    async fn watch_empty_service_subscribes() {
        let checker = StaticChecker::new();
        let mut stream = checker.watch("").await.unwrap();
        assert_eq!(stream.next().await.unwrap(), Status::Serving);
    }

    // Same shared-Sender invariant as the empty-service case below, but
    // for a registered non-empty name. A regression in `set_status` that
    // swapped `or_insert_with` for `insert` would break this silently.
    #[tokio::test]
    async fn concurrent_watchers_of_registered_service_share_a_sender() {
        let checker = Arc::new(StaticChecker::with_services(["acme.A"]));
        let mut a = checker.watch("acme.A").await.unwrap();
        let mut b = checker.watch("acme.A").await.unwrap();

        assert_eq!(a.next().await.unwrap(), Status::Serving);
        assert_eq!(b.next().await.unwrap(), Status::Serving);

        checker.set_status("acme.A", Status::NotServing);
        let a_next = tokio::time::timeout(std::time::Duration::from_secs(1), a.next())
            .await
            .expect("subscriber A did not receive update")
            .unwrap();
        let b_next = tokio::time::timeout(std::time::Duration::from_secs(1), b.next())
            .await
            .expect("subscriber B did not receive update")
            .unwrap();
        assert_eq!(a_next, Status::NotServing);
        assert_eq!(b_next, Status::NotServing);
    }

    // Regression test: earlier code inserted a fresh Sender on every
    // watch("") call, orphaning prior subscribers.
    #[tokio::test]
    async fn concurrent_watchers_of_empty_service_share_a_sender() {
        let checker = Arc::new(StaticChecker::new());
        let mut a = checker.watch("").await.unwrap();
        let mut b = checker.watch("").await.unwrap();

        // Both see the initial value.
        assert_eq!(a.next().await.unwrap(), Status::Serving);
        assert_eq!(b.next().await.unwrap(), Status::Serving);

        // A single update must reach both subscribers.
        checker.set_status("", Status::NotServing);
        let a_next = tokio::time::timeout(std::time::Duration::from_secs(1), a.next())
            .await
            .expect("subscriber A did not receive update")
            .unwrap();
        let b_next = tokio::time::timeout(std::time::Duration::from_secs(1), b.next())
            .await
            .expect("subscriber B did not receive update")
            .unwrap();
        assert_eq!(a_next, Status::NotServing);
        assert_eq!(b_next, Status::NotServing);
    }

    #[tokio::test]
    async fn set_same_status_does_not_notify() {
        let checker = StaticChecker::with_services(["acme.A"]);
        let mut stream = checker.watch("acme.A").await.unwrap();
        assert_eq!(stream.next().await.unwrap(), Status::Serving);

        // No change → no notification.
        checker.set_status("acme.A", Status::Serving);
        tokio::select! {
            item = stream.next() => panic!("unexpected notification on no-op set_status: {item:?}"),
            () = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
        }
    }

    #[tokio::test]
    async fn services_lists_every_registered_name() {
        let checker = StaticChecker::with_services(["acme.A", "acme.B"]);
        checker.set_status("acme.C", Status::NotServing);

        let mut names = checker.services();
        names.sort();
        // The whole-process "" entry is always present.
        assert_eq!(names, vec!["", "acme.A", "acme.B", "acme.C"]);
    }

    #[tokio::test]
    async fn shutdown_flips_whole_process_entry() {
        let checker = StaticChecker::new();
        assert_eq!(checker.check("").await.unwrap(), Status::Serving);
        checker.shutdown();
        assert_eq!(checker.check("").await.unwrap(), Status::NotServing);
    }

    // Cancelling a Watch RPC must release the underlying watch::Receiver
    // so the Sender stops holding state for a dead subscriber.
    #[tokio::test]
    async fn dropping_watch_stream_releases_subscriber() {
        let checker = StaticChecker::with_services(["acme.A"]);

        let receiver_count_before = {
            let services = checker.services.lock().unwrap();
            services.get("acme.A").unwrap().receiver_count()
        };

        let stream = checker.watch("acme.A").await.unwrap();
        let receiver_count_during = {
            let services = checker.services.lock().unwrap();
            services.get("acme.A").unwrap().receiver_count()
        };
        assert_eq!(receiver_count_during, receiver_count_before + 1);

        drop(stream);
        // `WatchStream` drops its `Receiver` synchronously, so the
        // `Sender` observes the decrement immediately.
        let receiver_count_after = {
            let services = checker.services.lock().unwrap();
            services.get("acme.A").unwrap().receiver_count()
        };
        assert_eq!(receiver_count_after, receiver_count_before);
    }

    #[tokio::test]
    async fn concurrent_set_status_does_not_panic() {
        let checker = Arc::new(StaticChecker::new());
        let mut handles = Vec::new();
        for i in 0..50 {
            let c = Arc::clone(&checker);
            handles.push(tokio::spawn(async move {
                let status = if i % 2 == 0 {
                    Status::Serving
                } else {
                    Status::NotServing
                };
                c.set_status("acme.race", status);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        let final_status = checker.check("acme.race").await.unwrap();
        assert!(matches!(final_status, Status::Serving | Status::NotServing));
    }
}
