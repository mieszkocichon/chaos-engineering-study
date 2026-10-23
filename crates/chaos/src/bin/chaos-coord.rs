//! Experiment-cell coordinator entry point – port of `runner/coordinator.js`.
//! Env contract: specs/001-rust-rewrite/contracts/cli-env.md.

#[tokio::main]
async fn main() {
    let cfg = chaos::coordinator::Config::from_env().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    if let Err(e) = chaos::coordinator::run_cell(&cfg).await {
        eprintln!("{e}");
        std::process::exit(1);
    }
    std::process::exit(0);
}
