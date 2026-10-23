use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::experiment::backend::{BackendPlan, ClusterBackend, PrepareContext, RunStatus};
use crate::generate::{self, SwarmOpts};

const DISAPPEARED_AFTER: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, Default)]
pub struct SwarmBackend;

impl SwarmBackend {
    fn docker(&self, args: &[&str]) -> Command {
        let mut c = Command::new("docker");
        c.args(args);
        c
    }

    fn docker_text(&self, args: &[&str]) -> String {
        self.docker(args)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default()
    }
}

impl ClusterBackend for SwarmBackend {
    fn name(&self) -> &'static str {
        "swarm"
    }

    fn prepare(&self, ctx: &PrepareContext) -> Result<BackendPlan, String> {
        let scale = ctx.cell.scale.unwrap_or(20);
        let seed = ctx.cell.seeds.first().copied().unwrap_or(1000);
        let seed_str = seed.to_string();

        let opts = SwarmOpts {
            scale,
            fault: &ctx.cell.fault,
            strategy: &ctx.cell.strategy,
            seed: &seed_str,
            seeds: Some(&seed_str),
            registry: None,
            rps: ctx.cell.rps,
            duration_sec: ctx.cell.duration_sec,
            warmup_sec: ctx.cell.warmup_sec,
            health_timeout_ms: ctx.cell.health_timeout_ms,
            config_dir_host: Some(ctx.config_dir_host.to_string()),
            results_dir_host: Some(ctx.results_dir_host.to_string()),
            image: Some(ctx.image.to_string()),
            extra_env: if ctx.cell.env.is_empty() { None } else { Some(ctx.cell.env.clone()) },
            stack_deploy: true,
        };

        let (_, yaml) = generate::swarm_with_opts(&opts)?;
        let stack_file = PathBuf::from(format!("docker-compose-swarm-{}.yml", ctx.project));
        std::fs::write(&stack_file, &yaml)
            .map_err(|e| format!("{}: {e}", stack_file.display()))?;

        let stack_name = format!("chaos_{}", ctx.project);
        Ok(BackendPlan {
            file_path: stack_file,
            project_or_stack: stack_name,
            network_driver: "overlay".into(),
        })
    }

    fn start(&self, plan: &BackendPlan) -> Result<(), String> {
        let stack = &plan.project_or_stack;
        // Clean up previous stack
        let _ = self.docker(&["stack", "rm", stack]).output();

        // Wait until services from previous stack are gone (up to 60s)
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let out = self.docker_text(&["service", "ls", "--filter", &format!("name={stack}"), "--format", "{{.Name}}"]);
            if out.trim().is_empty() || Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        std::thread::sleep(Duration::from_secs(3)); // brief grace period for overlay network cleanup

        let file_str = plan.file_path.to_str().unwrap();
        let deploy = self.docker(&["stack", "deploy", "-c", file_str, stack]).output()
            .map_err(|e| format!("docker stack deploy: {e}"))?;
        if !deploy.status.success() {
            return Err(format!("docker stack deploy failed: {}", String::from_utf8_lossy(&deploy.stderr).trim()));
        }
        Ok(())
    }

    fn check_status(&self, plan: &BackendPlan, started: Instant) -> Result<RunStatus, String> {
        let stack = &plan.project_or_stack;
        let coord_service = format!("{stack}_coordinator");
        let out = self.docker_text(&["service", "ps", &coord_service, "--format", "{{.CurrentState}}"]);
        let status = out.lines().next().unwrap_or("").trim();

        if status.contains("Complete") || status.contains("Shutdown") {
            return Ok(RunStatus::Exited(0));
        }
        if status.contains("Failed") || status.contains("Rejected") {
            return Ok(RunStatus::Exited(1));
        }
        if status.is_empty() && started.elapsed() > DISAPPEARED_AFTER {
            return Ok(RunStatus::Disappeared);
        }
        Ok(RunStatus::Running(status.to_string()))
    }

    fn stop_service(&self, plan: &BackendPlan, target: &str) -> Result<(), String> {
        let stack = &plan.project_or_stack;
        let service = format!("{stack}_{target}");
        // In Docker Swarm, scaling a service to 0 stops all its tasks
        let res = self.docker(&["service", "scale", &format!("{service}=0")]).output()
            .map_err(|e| format!("docker service scale {service}=0: {e}"))?;
        if !res.status.success() {
            return Err(format!("failed to stop {target}: {}", String::from_utf8_lossy(&res.stderr).trim()));
        }
        Ok(())
    }

    fn logs_tail(&self, plan: &BackendPlan, lines: usize) -> String {
        let stack = &plan.project_or_stack;
        let coord_service = format!("{stack}_coordinator");
        let lines_str = lines.to_string();
        self.docker_text(&["service", "logs", "--tail", &lines_str, &coord_service])
    }

    fn teardown(&self, plan: &BackendPlan) -> Result<(), String> {
        let stack = &plan.project_or_stack;
        let _ = self.docker(&["stack", "rm", stack]).output();
        let _ = std::fs::remove_file(&plan.file_path);
        Ok(())
    }
}
