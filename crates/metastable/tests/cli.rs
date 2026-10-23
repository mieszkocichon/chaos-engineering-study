//! US1 integration test (T026): every CLI invocation recorded from the JS reference
//! must produce byte-identical stdout from the Rust binaries.

use std::process::Command;

#[test]
fn cli_stdout_is_byte_identical_to_js() {
    let path = format!("{}/tests/golden/cli.json", env!("CARGO_MANIFEST_DIR"));
    let cases: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(cases.len() >= 30);
    for c in cases {
        let bin = c["bin"].as_str().unwrap();
        let exe = match bin {
            "run-m1" => env!("CARGO_BIN_EXE_run-m1"),
            "run-m2" => env!("CARGO_BIN_EXE_run-m2"),
            "sweep-hysteresis" => env!("CARGO_BIN_EXE_sweep-hysteresis"),
            other => panic!("unknown bin {other}"),
        };
        let args: Vec<&str> = c["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
        let out = Command::new(exe).args(&args).output().unwrap();
        assert!(out.status.success(), "{bin} {args:?} failed");
        let got = String::from_utf8(out.stdout).unwrap();
        let expected = c["stdout"].as_str().unwrap();
        if got != expected {
            let line = got.lines().zip(expected.lines()).position(|(a, b)| a != b);
            panic!(
                "{bin} {args:?}: stdout differs (first differing line {line:?})\n--- rust ---\n{}\n--- js ---\n{}",
                &got[..got.len().min(1500)],
                &expected[..expected.len().min(1500)]
            );
        }
    }
}
