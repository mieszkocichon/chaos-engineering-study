use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::experiment::backend::{BackendPlan, ClusterBackend, PrepareContext, RunStatus};
use crate::generate;

const DISAPPEARED_AFTER: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, Default)]
pub struct ComposeBackend;

impl ComposeBackend {
    fn compose(&self, plan: &BackendPlan, args: &[&str]) -> Command {
        let mut c = Command::new("docker");
        c.args(["compose", "-p", &plan.project_or_stack, "-f", plan.file_path.to_str().unwrap()]).args(args);
        c
    }

    fn compose_text(&self, plan: &BackendPlan, args: &[&str]) -> String {
        self.compose(plan, args)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default()
    }
}

impl ClusterBackend for ComposeBackend {
    fn name(&self) -> &'static str {
        "compose"
    }

    fn prepare(&self, ctx: &PrepareContext) -> Result<BackendPlan, String> {
        let topo_path = ctx.config_dir.join("topologies").join(format!("{}.json", ctx.cell.topology));
        let topo: Value = std::fs::read_to_string(&topo_path)
            .map_err(|e| format!("{}: {e}", topo_path.display()))
            .and_then(|s| serde_json::from_str(&s).map_err(|e| format!("{}: {e}", topo_path.display())))?;
        let opts = ctx.cell.compose_opts(
            ctx.seeds_str,
            ctx.project,
            ctx.image.to_string(),
            ctx.results_dir_host.to_string(),
            ctx.config_dir_host.to_string(),
        );
        let compose_file = PathBuf::from(format!("docker-compose-local-{}.yml", ctx.project));
        std::fs::write(&compose_file, generate::compose(&topo, &opts))
            .map_err(|e| format!("{}: {e}", compose_file.display()))?;

        Ok(BackendPlan {
            file_path: compose_file,
            project_or_stack: ctx.project.to_string(),
            network_driver: ctx.cell.network.clone().unwrap_or_else(|| "bridge".into()),
        })
    }

    fn start(&self, plan: &BackendPlan) -> Result<(), String> {
        let _ = self.compose(plan, &["down", "--remove-orphans"]).output();
        let up = self.compose(plan, &["up", "-d"]).output().map_err(|e| format!("docker compose up: {e}"))?;
        if !up.status.success() {
            return Err(format!("docker compose up failed: {}", String::from_utf8_lossy(&up.stderr).trim()));
        }
        Ok(())
    }

    fn check_status(&self, plan: &BackendPlan, started: Instant) -> Result<RunStatus, String> {
        let out = self.compose_text(plan, &["ps", "--all", "coordinator", "--format", "{{.Status}}"]);
        let status = out.lines().next().unwrap_or("").trim();
        if status.contains("Exit") {
            let code = status.split_once('(').and_then(|s| s.1.split_once(')')).and_then(|s| s.0.parse().ok()).unwrap_or(0);
            return Ok(RunStatus::Exited(code));
        }
        if status.is_empty() && started.elapsed() > DISAPPEARED_AFTER {
            return Ok(RunStatus::Disappeared);
        }
        Ok(RunStatus::Running(status.to_string()))
    }

    fn stop_service(&self, plan: &BackendPlan, target: &str) -> Result<(), String> {
        let res = self.compose(plan, &["stop", target]).output().map_err(|e| format!("docker compose stop {target}: {e}"))?;
        if !res.status.success() {
            return Err(format!("failed to stop {target}: {}", String::from_utf8_lossy(&res.stderr).trim()));
        }
        Ok(())
    }

    fn logs_tail(&self, plan: &BackendPlan, lines: usize) -> String {
        let lines_str = lines.to_string();
        self.compose_text(plan, &["logs", "--tail", &lines_str, "coordinator"])
    }

    fn teardown(&self, plan: &BackendPlan) -> Result<(), String> {
        let _ = self.compose(plan, &["down", "--remove-orphans"]).output();
        let _ = std::fs::remove_file(&plan.file_path);
        Ok(())
    }
}
