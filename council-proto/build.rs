fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .compile_protos(&["../proto/council/v1/council.proto"], &["../proto"])?;
    Ok(())
}
