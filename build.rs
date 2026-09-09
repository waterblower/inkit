use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=.local/dev-server-path");
    let marker =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join(".local/dev-server-path");
    // Only local debug builds use the opt-in marker written by build_native.sh.
    // Release artifacts never contain a developer's filesystem path.
    let path = if env::var("PROFILE").as_deref() == Ok("debug") {
        fs::read_to_string(marker).unwrap_or_default()
    } else {
        String::new()
    };
    println!("cargo:rustc-env=INKIT_DEV_SERVER={}", path.trim());
}
