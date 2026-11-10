use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::files::hash_bytes;
use crate::store::{atomic_json, read_json};

#[derive(Serialize, Deserialize)]
pub struct ProcessIdentity {
    pid: u32,
    signature: String,
}

impl ProcessIdentity {
    pub fn save(pid: Option<u32>, directory: &Path) -> Result<(), String> {
        if let Some(identity) = pid.and_then(Self::inspect) {
            atomic_json(&directory.join("process.json"), &identity)?;
        }
        Ok(())
    }

    fn inspect(pid: u32) -> Option<Self> {
        let output = std::process::Command::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "lstart=", "-o", "command="])
            .output()
            .ok()?;
        if !output.status.success() || output.stdout.is_empty() {
            return None;
        }
        Some(Self {
            pid,
            signature: hash_bytes(&output.stdout),
        })
    }

    fn stop_if_same(&self) {
        if Self::inspect(self.pid).is_some_and(|current| current.signature == self.signature) {
            crate::process::terminate_group(Some(self.pid));
        }
    }
}

pub fn stop_interrupted_processes(directory: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_dir()
            || !entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("trial-"))
        {
            continue;
        }
        let trial = entry.path();
        for path in [
            trial.join("client/process.json"),
            trial.join("judge/process.json"),
        ] {
            stop_saved_process(&path)?;
        }
        let checks = trial.join("checks");
        if checks.is_dir() {
            for check in std::fs::read_dir(checks).map_err(|e| e.to_string())? {
                let check = check.map_err(|e| e.to_string())?;
                if check.file_type().map_err(|e| e.to_string())?.is_dir() {
                    stop_saved_process(&check.path().join("process.json"))?;
                }
            }
        }
    }
    Ok(())
}

fn stop_saved_process(path: &Path) -> Result<(), String> {
    if path.is_file() {
        let identity: ProcessIdentity = read_json(path)?;
        identity.stop_if_same();
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;

    #[test]
    fn recovery_checks_process_identity_before_stopping_an_orphan() {
        let directory = tempfile::tempdir().unwrap();
        let evidence = directory.path().join("trial-1/client");
        std::fs::create_dir_all(&evidence).unwrap();
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .unwrap();
        let identity = ProcessIdentity::inspect(child.id()).unwrap();
        let wrong = ProcessIdentity {
            pid: identity.pid,
            signature: "old process".into(),
        };
        wrong.stop_if_same();
        assert!(child.try_wait().unwrap().is_none());
        atomic_json(&evidence.join("process.json"), &identity).unwrap();
        stop_interrupted_processes(directory.path()).unwrap();
        assert!(!child.wait().unwrap().success());
    }
}
