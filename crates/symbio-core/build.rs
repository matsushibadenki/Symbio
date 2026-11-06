use sha2::{Digest, Sha256};
fn main() {
    println!("cargo:rerun-if-env-changed=RUSTC");
    for (input, output) in [
        ("TARGET", "SYMBIO_BUILD_TARGET"),
        ("OPT_LEVEL", "SYMBIO_BUILD_OPT"),
        ("CARGO_CFG_TARGET_FEATURE", "SYMBIO_TARGET_FEATURES"),
    ] {
        println!("cargo:rerun-if-env-changed={input}");
        println!(
            "cargo:rustc-env={output}={}",
            std::env::var(input).expect("Cargo build configuration")
        );
    }
    let mut files = vec![std::path::PathBuf::from("Cargo.toml"), "build.rs".into()];
    fn collect(path: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(path).expect("source directory") {
            let entry = entry.expect("source entry");
            let ty = entry.file_type().expect("source type");
            assert!(!ty.is_symlink(), "source symlinks are not fingerprinted");
            if ty.is_dir() {
                collect(&entry.path(), files);
            } else if entry
                .path()
                .extension()
                .is_some_and(|s| s == "rs" || s == "py")
            {
                files.push(entry.path());
            }
        }
    }
    collect(std::path::Path::new("src"), &mut files);
    files.sort();
    println!("cargo:rerun-if-changed=src");
    let mut hash = Sha256::new();
    hash.update(b"symbio-core-source-v2\0");
    for file in files {
        println!("cargo:rerun-if-changed={}", file.display());
        let name = file.to_str().expect("source name").replace('\\', "/");
        let bytes = std::fs::read(&file).expect("source bytes");
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    println!(
        "cargo:rustc-env=SYMBIO_CORE_SOURCE_SHA256={:x}",
        hash.finalize()
    );
    let rustc = std::env::var_os("RUSTC").expect("Cargo sets RUSTC");
    let output = std::process::Command::new(rustc)
        .arg("--version")
        .output()
        .expect("compiler version");
    assert!(output.status.success());
    let version = String::from_utf8(output.stdout).expect("UTF-8 compiler version");
    println!("cargo:rustc-env=SYMBIO_RUSTC_VERSION={}", version.trim());
}
