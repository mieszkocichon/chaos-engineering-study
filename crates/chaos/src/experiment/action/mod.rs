use crate::experiment::backend::{BackendPlan, ClusterBackend};

pub trait ActionKind: Send + Sync {
    fn name(&self) -> &'static str;
    fn execute(&self, backend: &dyn ClusterBackend, plan: &BackendPlan, target: &str) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StopServiceAction;

impl ActionKind for StopServiceAction {
    fn name(&self) -> &'static str {
        "stop-service"
    }

    fn execute(&self, backend: &dyn ClusterBackend, plan: &BackendPlan, target: &str) -> Result<(), String> {
        backend.stop_service(plan, target)
    }
}

static STOP_SERVICE: StopServiceAction = StopServiceAction;

pub fn find_action_kind(name: &str) -> Option<&'static dyn ActionKind> {
    match name {
        "stop-service" => Some(&STOP_SERVICE),
        _ => None,
    }
}
