use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::experiment::cell::Cell;

pub mod compose;
pub mod swarm;

pub use compose::ComposeBackend;
pub use swarm::SwarmBackend;

pub struct PrepareContext<'a> {
    pub cell: &'a Cell,
    pub seeds_str: &'a str,
    pub project: &'a str,
    pub image: &'a str,
    pub results_dir_host: &'a str,
    pub results_path: &'a Path,
    pub config_dir: &'a Path,
    pub config_dir_host: &'a str,
}

#[derive(Clone, Debug)]
pub struct BackendPlan {
    pub file_path: PathBuf,
    pub project_or_stack: String,
    pub network_driver: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunStatus {
    Running(String),
    Exited(i32),
    Disappeared,
}

pub trait ClusterBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn prepare(&self, ctx: &PrepareContext) -> Result<BackendPlan, String>;
    fn start(&self, plan: &BackendPlan) -> Result<(), String>;
    fn check_status(&self, plan: &BackendPlan, started: Instant) -> Result<RunStatus, String>;
    fn stop_service(&self, plan: &BackendPlan, target: &str) -> Result<(), String>;
    fn logs_tail(&self, plan: &BackendPlan, lines: usize) -> String;
    fn teardown(&self, plan: &BackendPlan) -> Result<(), String>;
}

static COMPOSE_BACKEND: ComposeBackend = ComposeBackend;
static SWARM_BACKEND: SwarmBackend = SwarmBackend;

pub fn find_backend(name: &str) -> Option<&'static dyn ClusterBackend> {
    match name {
        "compose" => Some(&COMPOSE_BACKEND),
        "swarm" => Some(&SWARM_BACKEND),
        _ => None,
    }
}
