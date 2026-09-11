//! Stamps the build so the About screen can answer a support question without guessing.
//!
//! Re-run when the client's own sources change, which is exactly when a new build exists.

use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("cargo::rerun-if-changed=src");

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or(0);

    println!("cargo::rustc-env=AURUM_BUILD_TIME={millis}");
}
