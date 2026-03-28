fn main() {
    connectrpc_build::Config::new()
        .files(&["../eliza/proto/connectrpc/eliza/v1/eliza.proto"])
        .includes(&["../eliza/proto/"])
        .include_file("_connectrpc.rs")
        .compile()
        .expect("failed to compile protos");
}
