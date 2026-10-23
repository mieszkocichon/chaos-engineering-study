use std::fs::{File, TryLockError};
use std::path::Path;

/// Exclusivity per compose project prefix: two runs with the same prefix would tear down each
/// other's clusters (`<prefix>1`, `<prefix>2`, ...). An OS file lock is released when the process
/// dies, so Ctrl+C or a crash never leaves a stale lock behind.
pub struct LockGuard {
    _file: File,
}

impl LockGuard {
    pub fn acquire(dir: &Path, project_prefix: &str) -> Result<Self, String> {
        let path = dir.join(format!(".chaos-run-{project_prefix}.lock"));
        let file = File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(format!(
                "another chaos-run with project prefix '{project_prefix}' is active (lock: {})",
                path.display()
            )),
            Err(TryLockError::Error(e)) => Err(format!("{}: {e}", path.display())),
        }
    }
}
