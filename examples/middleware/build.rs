fn main() {
    connectrpc_build::Config::new()
        .files(&["proto/anthropic/connectrpc/middleware_demo/v1/secret.proto"])
        .includes(&["proto/"])
        .include_file("_connectrpc.rs")
        .compile()
        .unwrap();
}
