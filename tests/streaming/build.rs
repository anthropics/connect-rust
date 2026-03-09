fn main() {
    connectrpc_build::Config::new()
        .files(&["proto/echo.proto"])
        .includes(&["proto/"])
        .include_file("_connectrpc.rs")
        .compile()
        .unwrap();
}
