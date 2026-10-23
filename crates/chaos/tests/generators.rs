//! T064: `chaos-gen` produces the same files as the JS generators, except the intended
//! differences of research R13, applied to the JS output by `expected()` below.

mod common;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::golden;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

fn list(dir: &Path, base: &Path, out: &mut BTreeMap<String, String>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            list(&p, base, out);
        } else {
            let k = p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
            out.insert(k, std::fs::read_to_string(&p).unwrap());
        }
    }
}

/// The intended JS → Rust differences (research R13), and nothing else.
fn expected(js: &str) -> String {
    let mut s = js
        .replace(r#"["node", "runner/service-runner.js"]"#, r#"["chaos-svc"]"#)
        .replace(r#"["node", "runner/coordinator.js"]"#, r#"["chaos-coord"]"#)
        .replace("chaos-experiment:latest", "chaos-experiment-rs:latest")
        .replace(
            r#"["CMD", "wget", "-q", "http://localhost:4000/health", "-O", "/dev/null"]"#,
            r#"["CMD", "curl", "-fsS", "http://localhost:4000/health"]"#,
        );
    // ["CMD", "node", "-e", "fetch('http://localhost:<port>/health')…"]
    while let Some(i) = s.find(r#"["CMD", "node", "-e", "fetch('http://localhost:"#) {
        let rest = &s[i..];
        let end = rest.find("\"]").unwrap() + 2;
        let port: String = rest[r#"["CMD", "node", "-e", "fetch('http://localhost:"#.len()..].chars().take_while(char::is_ascii_digit).collect();
        s.replace_range(i..i + end, &format!(r#"["CMD", "curl", "-fsS", "http://localhost:{port}/health"]"#));
    }
    s
}

#[test]
fn generators_match_js_modulo_intended_differences() {
    let repo = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    for c in golden("generators.json").as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let tmp = std::env::temp_dir().join(format!("chaos-gen-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let w = tmp.join("w");
        copy_dir(&repo.join("when-gossip-goes-silent/config"), &w.join("config"));
        std::fs::create_dir_all(tmp.join("root")).unwrap();
        let mut before = BTreeMap::new();
        list(&w, &w, &mut before);

        let script = c["script"].as_str().unwrap();
        let cwd = if script.starts_with("ring") { tmp.join("root") } else { w.clone() };
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_chaos-gen"));
        cmd.arg(script).current_dir(&cwd);
        for a in c["args"].as_array().unwrap() {
            cmd.arg(a.as_str().unwrap());
        }
        for (k, v) in c.get("env").and_then(|e| e.as_object()).into_iter().flatten() {
            cmd.env(k, v.as_str().unwrap());
        }
        let out = cmd.output().unwrap();
        assert!(out.status.success(), "{name}: {}", String::from_utf8_lossy(&out.stderr));

        let mut after = BTreeMap::new();
        list(&w, &w, &mut after);
        let mut root = BTreeMap::new();
        list(&tmp.join("root"), &tmp.join("root"), &mut root);
        for (k, v) in root {
            after.insert(format!("../root/{k}"), v);
        }
        let changed: BTreeMap<_, _> = after.into_iter().filter(|(k, v)| before.get(k) != Some(v)).collect();
        let exp: BTreeMap<String, String> =
            c["files"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), expected(v.as_str().unwrap()))).collect();
        assert_eq!(changed.keys().collect::<Vec<_>>(), exp.keys().collect::<Vec<_>>(), "{name}: written files");
        for (k, v) in &exp {
            let got = &changed[k];
            if got != v {
                let line = got.lines().zip(v.lines()).position(|(a, b)| a != b);
                panic!("{name}: {k} differs at line {line:?}\n--- rust ---\n{}\n--- expected ---\n{}",
                    got.lines().nth(line.unwrap_or(0)).unwrap_or(""), v.lines().nth(line.unwrap_or(0)).unwrap_or(""));
            }
        }
        // results dirs created by the compose generator
        for d in c["dirs"].as_array().unwrap() {
            assert!(w.join(d.as_str().unwrap()).is_dir(), "{name}: dir {d} not created");
        }
        // stdout: same lines (root generators recorded repo-relative), modulo mesh's absolute "Wrote" path
        let norm = |s: &str| {
            s.lines().map(|l| l.split(" Wrote ").next().unwrap().to_string()).collect::<Vec<_>>()
        };
        assert_eq!(norm(&String::from_utf8_lossy(&out.stdout)), norm(c["stdout"].as_str().unwrap()), "{name}: stdout");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

#[test]
fn compose_reports_missing_topology_like_js() {
    let tmp = std::env::temp_dir().join(format!("chaos-gen-missing-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_chaos-gen")).args(["compose", "nope"]).current_dir(&tmp).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("[generate] ERROR: cannot read topology file:"));
    let _ = std::fs::remove_dir_all(&tmp);
}
