fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::compile_protos("../rpc/proto/bench.proto")?;
    tonic_prost_build::compile_protos("../rpc/proto/fortune.proto")?;
    Ok(())
}
