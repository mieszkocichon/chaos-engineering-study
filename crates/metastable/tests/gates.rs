//! US1 integration test (T034): `metastable-failures/scripts/gate-a.sh` driven by the Rust
//! binary prints exactly what it printed with the JS reference (golden/gate-a.txt).

use std::process::Command;

#[test]
fn gate_a_script_output_matches_js() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../metastable-failures");
    let out = Command::new("sh")
        .arg("scripts/gate-a.sh")
        .current_dir(root)
        .env("M1", env!("CARGO_BIN_EXE_run-m1"))
        .output()
        .expect("sh + md5sum available (Linux container)");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let expected = include_str!("golden/gate-a.txt");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), expected);
}
