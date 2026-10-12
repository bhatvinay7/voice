fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                "../../../proto/scheduler.proto",
                "../../libs/orchestrator/proto/tools.proto",
            ],
            &[
                "../../../proto",
                "../../libs/orchestrator/proto",
            ],
        )?;
    Ok(())
}
