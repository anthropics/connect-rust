//! View-response filter server: if no sensitive field is set, return
//! the request `OwnedView` directly (re-encodes from the borrowed view
//! via `ViewEncode`, no per-field allocation). Otherwise convert to
//! owned, scrub, and return owned.

use connectrpc::{ConnectRpcService, MaybeBorrowed, RequestContext, ServiceResult};

use rpc_bench::filter::*;

struct Impl;

impl FilterService for Impl {
    async fn redact(
        &self,
        _ctx: RequestContext,
        request: OwnedRecordView,
    ) -> ServiceResult<MaybeBorrowed<Record, OwnedRecordView>> {
        if !has_sensitive(&request) {
            return Ok(MaybeBorrowed::borrowed(request).into());
        }
        let mut owned = request.to_owned_message();
        scrub(&mut owned);
        Ok(MaybeBorrowed::owned(owned).into())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service = ConnectRpcService::new(FilterServiceServer::new(Impl));
    let bound = connectrpc::server::Server::bind("127.0.0.1:0").await?;
    println!("{}", bound.local_addr()?);
    tokio::select! {
        result = bound.serve_with_service(service) => result?,
        _ = tokio::signal::ctrl_c() => {}
    }
    Ok(())
}
