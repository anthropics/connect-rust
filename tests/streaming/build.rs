fn main() {
    connectrpc_build::Config::new()
        .files(&["proto/echo.proto"])
        .includes(&["proto/"])
        .include_file("_connectrpc.rs")
        .view_encode(true)
        .generic_response_type(true)
        .compile()
        .unwrap();
}
