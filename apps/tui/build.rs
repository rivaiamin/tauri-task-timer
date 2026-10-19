use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let root = manifest.join("../..");
    let source = manifest.join("src/orchestrator_adapter.mjs");
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"))
        .join("orchestrator-adapter.mjs");

    println!("cargo:rerun-if-changed=src/orchestrator_adapter.mjs");
    println!("cargo:rerun-if-env-changed=PATH");

    let status = Command::new("pnpm")
        .args([
            "exec",
            "esbuild",
            source.to_str().expect("adapter path is UTF-8"),
            "--bundle",
            "--platform=node",
            "--format=esm",
        ])
        .arg(format!("--outfile={}", output.display()))
        .current_dir(root)
        .status()
        .expect("pnpm and Node.js are required to bundle the Paseo SDK adapter");
    assert!(status.success(), "failed to bundle the Paseo SDK adapter");
}
