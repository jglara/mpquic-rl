fn main() -> Result<(), Box<dyn std::error::Error>> {
    capnpc::CompilerCommand::new()
        .src_prefix("src")
        .file("src/data.capnp")
        .run().expect("schema compiler command");

    Ok(())
}