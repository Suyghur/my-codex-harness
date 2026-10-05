use std::{env, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=RUSTC");
    let compiler = env::var_os("RUSTC").expect("Cargo must provide RUSTC");
    let output = Command::new(compiler)
        .arg("--version")
        .output()
        .expect("cannot obtain build compiler version");
    assert!(output.status.success(), "rustc --version failed");
    let version = String::from_utf8(output.stdout).expect("invalid rustc version encoding");
    println!("cargo:rustc-env=HARNESS_RUST_VERSION={}", version.trim());
}
