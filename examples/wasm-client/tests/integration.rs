use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn say_via_fetch_transport() {
    let response = wasm_client_example::say("http://127.0.0.1:8080", "Hello!".into())
        .await
        .unwrap();
    assert!(
        response.starts_with("Hello"),
        "unexpected reply: {response}"
    );
}

#[wasm_bindgen_test]
async fn say_invalid_url() {
    let err = wasm_client_example::say("not a url", "Hello!".into())
        .await
        .unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("invalid"),
        "expected URL parse error, got: {msg}"
    );
}

#[wasm_bindgen_test]
async fn say_unreachable_server() {
    let err = wasm_client_example::say("http://127.0.0.1:1", "Hello!".into())
        .await
        .unwrap_err();
    let msg = format!("{err:?}");
    assert!(!msg.is_empty(), "expected a fetch error message");
}
