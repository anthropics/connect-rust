//! Wasm ConnectRPC client example.
//!
//! Demonstrates a `ClientTransport` backed by the browser Fetch API,
//! allowing generated ConnectRPC clients to run in `wasm32-unknown-unknown`
//! environments (browsers, web workers, etc.).
//!
//! ## Building
//!
//! ```bash
//! wasm-pack build examples/wasm-client --target web
//! ```

mod transport;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/_connectrpc.rs"));
}

use ::connectrpc::client::ClientConfig;
use proto::connectrpc::eliza::v1::*;
use wasm_bindgen::prelude::*;

/// Call the Eliza `Say` RPC from JavaScript.
///
/// ```js
/// import init, { say } from './pkg/wasm_client_example.js';
/// await init();
/// const reply = await say("http://localhost:8080", "Hello!");
/// console.log(reply);
/// ```
#[wasm_bindgen]
pub async fn say(base_url: &str, sentence: String) -> Result<String, JsError> {
    let config = ClientConfig::new(base_url.parse().map_err(JsError::from)?);
    let client = ElizaServiceClient::new(transport::FetchTransport, config);

    let response = client
        .say(SayRequest {
            sentence,
            ..Default::default()
        })
        .await
        .map_err(|e| JsError::new(&e.to_string()))?;

    Ok(response.into_owned().sentence)
}
