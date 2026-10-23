use futures::future::BoxFuture;
use serde_json::Value;

use crate::faults::{argv, Argv, FaultInjector, FaultKind, FaultLayer};

pub struct CascadeCrashFault;

impl FaultKind for CascadeCrashFault {
    fn name(&self) -> &'static str {
        "cascade-crash"
    }

    fn layer(&self) -> FaultLayer {
        FaultLayer::Process
    }

    fn commands(&self, _opts: &Value) -> Result<Vec<Argv>, String> {
        Ok(vec![])
    }

    fn default_recover_after_sec(&self) -> Option<f64> {
        Some(35.0)
    }

    fn execute<'a>(&'a self, injector: &'a FaultInjector, _opts: &'a Value) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            println!("[InfraFault] Executing SIGKILL on self...");
            if injector.is_recording() {
                injector.record_cmd(argv("SIGKILL self"));
                return;
            }
            // SAFETY: plain libc call on our own pid; the process dies here.
            unsafe {
                libc::kill(libc::getpid(), libc::SIGKILL);
            }
        })
    }
}
