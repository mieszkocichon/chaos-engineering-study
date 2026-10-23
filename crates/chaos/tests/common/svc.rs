#![allow(dead_code)]
//! Spawn real `chaos-svc` processes on localhost for integration tests.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub fn config_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../when-gossip-goes-silent/config"))
}

pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
}

/// A base port with `n` consecutive free ports after it (best effort).
pub fn free_base_port(n: u16) -> u16 {
    loop {
        let base = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        if base > 60000 {
            continue;
        }
        if (0..n).all(|i| TcpListener::bind(("127.0.0.1", base + i)).is_ok()) {
            return base;
        }
    }
}

pub struct Svc {
    pub child: Child,
    pub url: String,
}

impl Drop for Svc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Start one service; hosts resolve via `HOST_OVERRIDE=127.0.0.1` so topologies work locally.
pub fn spawn(id: &str, topology: &str, config: &PathBuf, base_port: u16, port: u16, strategy: &str, extra: &[(&str, &str)]) -> Svc {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_chaos-svc"));
    cmd.env("SERVICE_ID", id)
        .env("TOPOLOGY", topology)
        .env("FAULT_PROFILE", "no-wan")
        .env("STRATEGY", strategy)
        .env("SEED", "1000")
        .env("BASE_PORT", base_port.to_string())
        .env("CONFIG_DIR", config)
        .env("HOST_OVERRIDE", "127.0.0.1")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let svc = Svc { child: cmd.spawn().unwrap(), url: format!("http://127.0.0.1:{port}") };
    wait_healthy(&svc.url);
    svc
}

pub fn wait_healthy(url: &str) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if ureq_get(&format!("{url}/health")).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("{url} not healthy");
}

/// Blocking `GET` returning Ok on HTTP 200 (usable from inside a tokio test).
fn ureq_get(url: &str) -> Result<(), ()> {
    use std::io::{Read, Write};
    let rest = url.strip_prefix("http://").ok_or(())?;
    let (addr, path) = rest.split_once('/').map(|(a, p)| (a, format!("/{p}"))).unwrap_or((rest, "/".into()));
    let mut s = std::net::TcpStream::connect(addr).map_err(|_| ())?;
    s.set_read_timeout(Some(Duration::from_secs(2))).ok();
    write!(s, "GET {path} HTTP/1.0\r\nHost: {addr}\r\n\r\n").map_err(|_| ())?;
    let mut buf = [0u8; 32];
    let n = s.read(&mut buf).map_err(|_| ())?;
    if String::from_utf8_lossy(&buf[..n]).contains(" 200 ") { Ok(()) } else { Err(()) }
}
