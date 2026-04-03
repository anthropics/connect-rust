fn main() {
    connectrpc_build::Config::new()
        .files(&["proto/anthropic/connectrpc/tour/v1/number.proto"])
        .includes(&["proto/"])
        .include_file("_connectrpc.rs")
        .compile()
        .unwrap();
}
