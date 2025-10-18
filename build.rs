use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from("src/gen");
    tonic_prost_build::configure()
        .out_dir(&out_dir)
        .file_descriptor_set_path(out_dir.join("limitr_descriptor.bin"))
        .build_server(true)
        .build_client(false)
        .compile_protos(&["proto/limitr/v1/limitr.proto"], &["proto"])?;
    println!("cargo:rerun-if-changed=proto/limitr/v1/limitr.proto");
    Ok(())
}
