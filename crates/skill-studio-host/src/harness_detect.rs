//! [`ProcessSpawner`] over the real OS process, for the `harnesses` op's
//! `--version` probe and the desktop's `npx`/`git` mutations.

use crate::probe_cache::{ProbeCache, ProbeKey};
use std::ffi::OsString;
use std::io::Read;
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use skill_studio_core::ports::{CancelToken, ProcessOutput, ProcessSpawner, ProcessSpec};
use skill_studio_core::CoreError;

use crate::tools::is_executable_file;

/// How often [`RealProcessSpawner::run`] polls a running child for exit
/// while waiting for `ProcessSpec::timeout_ms`'s deadline.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// How long [`broken_node_reason`] lets `node --version` run. It runs on every
/// failed npx run, so a hanging `node` adds at most this long to the failure.
/// A healthy-but-slow start must not hit it either: on a loaded machine, or
/// on the first launch of a just-upgraded binary, a process start measured
/// up to 3.6 s, and a probe that gives up reports no cause.
const NODE_VERSION_TIMEOUT: Duration = Duration::from_secs(10);

/// Attempts and pause for [`spawn_retrying_busy`]: about half a second in all.
const BUSY_SPAWN_ATTEMPTS: u32 = 20;
const BUSY_SPAWN_BACKOFF: Duration = Duration::from_millis(25);

/// `command.spawn()`, retried while the program is "Text file busy"
/// (ETXTBSY). On Linux, exec of a file fails that way while any process still
/// holds it open for writing. A thread that has just written an executable
/// (an installer, a script, a test fixture) can have that descriptor copied
/// into a child that another thread forks at the same moment; the copy
/// closes when that child execs, within milliseconds. A real binary is
/// never busy for long, so a bounded retry is harmless and hides the race.
pub fn spawn_retrying_busy(command: &mut std::process::Command) -> std::io::Result<Child> {
    retry_when_busy(|| command.spawn())
}

fn retry_when_busy<T>(mut attempt: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut tries = 1;
    loop {
        match attempt() {
            Err(e)
                if e.kind() == std::io::ErrorKind::ExecutableFileBusy
                    && tries < BUSY_SPAWN_ATTEMPTS =>
            {
                tries += 1;
                std::thread::sleep(BUSY_SPAWN_BACKOFF);
            }
            result => return result,
        }
    }
}

/// Time allowed for EOF after process cleanup before stopping the readers.
const KILLED_READER_GRACE: Duration = Duration::from_millis(250);

/// Runs a child process with `std::process::Command` and waits for it to
/// exit, killing it if it outlives `ProcessSpec::timeout_ms`.
///
pub struct RealProcessSpawner {
    /// Directories searched to resolve a bare `ProcessSpec::program` and
    /// prepended to the child's `PATH`, ahead of this process's own. Empty
    /// for [`RealProcessSpawner::new`]: the CLI and MCP server already run
    /// under a terminal shell's full `PATH`, so they keep spawning exactly
    /// as before. Non-empty for [`RealProcessSpawner::with_search_path`],
    /// which the desktop app uses: launched from Finder, it inherits
    /// `launchd`'s minimal `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`), where
    /// neither `npx` nor the `node` its `#!/usr/bin/env node` shebang needs
    /// can be found.
    search_dirs: Vec<PathBuf>,
    isolated_home: Option<PathBuf>,
    clock: Arc<dyn skill_studio_core::ports::Clock>,
    probes: Mutex<ProbeCache>,
    reader_grace: Duration,
}

impl RealProcessSpawner {
    /// Builds the spawner with an in-memory version-probe cache.
    pub fn new() -> Self {
        RealProcessSpawner {
            search_dirs: Vec::new(),
            isolated_home: None,
            clock: Arc::new(crate::clock::SystemClock::new()),
            probes: Mutex::new(ProbeCache::default()),
            reader_grace: KILLED_READER_GRACE,
        }
    }

    /// Builds a production spawner whose cache is confined to the selected home.
    pub fn for_home(home: &Path, search_dirs: Vec<PathBuf>) -> Self {
        Self::with_search_path(search_dirs)
            .with_probe_cache(home.join(".skill-studio/version-probes.json"))
    }

    /// Reuses version probes across instances through an app-owned cache file.
    #[must_use]
    pub fn with_probe_cache(mut self, path: PathBuf) -> Self {
        self.probes = Mutex::new(ProbeCache::load(path));
        self
    }

    /// As [`RealProcessSpawner::new`], but resolving a bare `program` name
    /// against `search_dirs` first and prepending `search_dirs` to every
    /// child's `PATH` (unless `ProcessSpec::env` already sets `PATH`
    /// itself). Pass the same directories a `LoginShellToolLookup` probed,
    /// so the spawn agrees with whatever `find_binary` already promised the
    /// caller was there.
    pub fn with_search_path(search_dirs: Vec<PathBuf>) -> Self {
        RealProcessSpawner {
            search_dirs,
            ..Self::new()
        }
    }

    /// Confines inherited npm and Git configuration to an explicit home scope.
    #[must_use]
    pub fn with_isolated_home(mut self, home: PathBuf) -> Self {
        self.isolated_home = Some(home);
        self
    }

    fn isolate_environment(&self, command: &mut Command) {
        let Some(home) = &self.isolated_home else {
            return;
        };
        let keys: Vec<_> = std::env::vars_os()
            .map(|(key, _)| key)
            .chain(command.get_envs().map(|(key, _)| key.to_os_string()))
            .filter(|key| {
                let key = key.to_string_lossy().to_ascii_lowercase();
                key.starts_with("npm_config_")
                    || key == "git_config"
                    || key.starts_with("git_config_")
            })
            .collect();
        for key in keys {
            command.env_remove(key);
        }
        for (name, path) in [
            ("cache", home.join(".npm")),
            ("userconfig", home.join(".npmrc")),
            ("globalconfig", home.join(".npm-global/etc/npmrc")),
            ("prefix", home.join(".npm-global")),
        ] {
            command
                .env(format!("npm_config_{name}"), &path)
                .env(format!("NPM_CONFIG_{}", name.to_ascii_uppercase()), path);
        }
        command
            .env("HOME", home)
            .env("GIT_CONFIG_GLOBAL", home.join(".gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1");
    }

    /// Resolves `program` to an absolute path under `search_dirs` when it is
    /// a bare name (no `/`); falls back to `program` unchanged otherwise, or
    /// when no `search_dirs` entry has it (`std::process::Command` then
    /// resolves it against the child's own `PATH`, set below).
    fn resolve_program(&self, program: &str) -> PathBuf {
        if program.contains('/') {
            return PathBuf::from(program);
        }
        self.search_dirs
            .iter()
            .map(|dir| dir.join(program))
            .find(|candidate| is_executable_file(candidate))
            .unwrap_or_else(|| PathBuf::from(program))
    }

    /// `search_dirs` joined with this process's own `PATH`, for a child that
    /// needs to resolve a second binary itself (`npx`'s `#!/usr/bin/env
    /// node` shebang needs `node` on the child's `PATH`, not just its own
    /// `argv[0]` resolved).
    fn child_path(&self) -> OsString {
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        // A dir containing the PATH separator can't be represented in a
        // joined PATH string. Skip just that dir instead of letting
        // `join_paths` fail and falling back to `inherited` alone, which
        // would silently drop every other `search_dirs` entry too.
        let search_dirs = self
            .search_dirs
            .iter()
            .filter(|dir| !dir.as_os_str().to_string_lossy().contains(':'))
            .cloned();
        // An empty (unset or "") inherited PATH must contribute nothing, not
        // an empty path segment: `split_paths` on "" yields one empty
        // component, which `Command` resolves as the child's cwd - for
        // `npx` that is a project folder, not a directory to trust bare
        // names from.
        let inherited_dirs: Vec<PathBuf> = if inherited.is_empty() {
            Vec::new()
        } else {
            std::env::split_paths(&inherited).collect()
        };
        std::env::join_paths(search_dirs.chain(inherited_dirs)).unwrap_or(inherited)
    }

    /// The `PATH` `run` gives the child for `spec`.
    fn effective_path(&self, spec: &ProcessSpec) -> OsString {
        if let Some((_, value)) = spec.env.iter().find(|(key, _)| key == "PATH") {
            return OsString::from(value);
        }
        if self.search_dirs.is_empty() {
            return std::env::var_os("PATH").unwrap_or_default();
        }
        self.child_path()
    }

    /// For a failed `npx` run, appends one `Ran:` line naming the program,
    /// its argv, and the `node` on the child's `PATH`, so a wrong Node
    /// (Homebrew's instead of the user's mise) shows in the error. When that
    /// `node` cannot start at all, the output says so instead of passing on
    /// npx's raw dyld text. Never reads environment values or tokens.
    fn describe_failed_npx(
        &self,
        spec: &ProcessSpec,
        program: &Path,
        mut output: ProcessOutput,
    ) -> ProcessOutput {
        let is_npx = Path::new(&spec.program)
            .file_name()
            .is_some_and(|n| n == "npx");
        if !is_npx || output.status == Some(0) {
            return output;
        }
        let path = self.effective_path(spec);
        let node = std::env::split_paths(&path)
            .map(|dir| dir.join("node"))
            .find(|candidate| is_executable_file(candidate));
        if let Some(node) = &node {
            if let Some(broken) = broken_node_reason(node, &path) {
                output.stderr = format!("The Node at {} is broken: {broken}", node.display());
            }
        }
        let node_text = node.map_or_else(|| "not found".to_string(), |n| n.display().to_string());
        let mut line = format!("Ran: {}", program.display());
        for arg in &spec.args {
            line.push(' ');
            line.push_str(arg);
        }
        line.push_str(" (node: ");
        line.push_str(&node_text);
        line.push(')');
        if !output.stderr.is_empty() && !output.stderr.ends_with('\n') {
            output.stderr.push('\n');
        }
        output.stderr.push_str(&line);
        output
    }
}

/// The first dyld "Library not loaded" line from `node --version`, when
/// `node` fails to start for that reason. `None` for a working Node or any
/// other failure.
fn broken_node_reason(node: &Path, path: &OsString) -> Option<String> {
    let mut command = std::process::Command::new(node);
    command
        .arg("--version")
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = spawn_retrying_busy(&mut command).ok()?;
    let stderr_reader = child.stderr.take().map(spawn_drain::<ChildStderr>);
    let start = Instant::now();
    let exited = loop {
        match child_exited_unreaped(&child) {
            Ok(true) => break true,
            Ok(false) if start.elapsed() < NODE_VERSION_TIMEOUT => {
                std::thread::sleep(POLL_INTERVAL);
            }
            _ => break false,
        }
    };
    let tree_stopped = kill_process_tree(child.id());
    let _ = child.kill();
    let status = child.wait();
    let stderr = join_killed_reader(stderr_reader, KILLED_READER_GRACE);
    if !exited || !tree_stopped {
        return None;
    }
    let status = status.ok()?;
    if status.success() {
        return None;
    }
    String::from_utf8_lossy(&stderr)
        .lines()
        .find(|line| line.contains("Library not loaded"))
        .map(|line| line.trim().to_string())
}

impl Default for RealProcessSpawner {
    fn default() -> Self {
        RealProcessSpawner::new()
    }
}

/// A reader with an explicit stop signal. Must start before the
/// caller polls the child for exit: a child that writes more than the pipe
/// buffer (about 64 KB) and isn't read blocks on that write forever, so
/// `try_wait` would never see it exit and the caller would always hit its
/// deadline instead of its real completion.
struct PipeDrain {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<Vec<u8>>,
}

impl PipeDrain {
    fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }
}

fn spawn_drain<R: Read + AsFd + Send + 'static>(mut pipe: R) -> PipeDrain {
    let stop = Arc::new(AtomicBool::new(false));
    let cancelled = stop.clone();
    let handle = std::thread::spawn(move || {
        use rustix::event::{poll, PollFd, PollFlags, Timespec};
        let mut output = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            let mut descriptors = [PollFd::new(&pipe, PollFlags::IN)];
            let timeout = Timespec {
                tv_sec: 0,
                tv_nsec: 10_000_000,
            };
            match poll(&mut descriptors, Some(&timeout)) {
                Ok(0) => {
                    if cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    continue;
                }
                Ok(_) => {}
                Err(rustix::io::Errno::INTR) => continue,
                Err(_) => break,
            }
            match pipe.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => output.extend_from_slice(&buffer[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
            if cancelled.load(Ordering::Acquire) {
                break;
            }
        }
        output
    });
    PipeDrain { stop, handle }
}

#[cfg(unix)]
fn signal_process_group(pid: u32, signal: rustix::process::Signal) -> bool {
    rustix::process::Pid::from_raw(pid.cast_signed())
        .is_some_and(|pid| rustix::process::kill_process_group(pid, signal).is_ok())
}

#[cfg(unix)]
fn kill_process_group(pid: u32) -> bool {
    signal_process_group(pid, rustix::process::Signal::KILL)
}

#[cfg(unix)]
fn kill_process_tree(pid: u32) -> bool {
    kill_process_tree_before_collect(pid, || {})
}

fn kill_process_tree_before_collect(pid: u32, before_collect: impl FnOnce()) -> bool {
    let _ = signal_process_group(pid, rustix::process::Signal::STOP);
    before_collect();
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let Ok(mut system) = System::new() else {
        let _ = kill_process_group(pid);
        return false;
    };
    let mut descendants = vec![Pid::from_u32(pid)];
    let mut identities = Vec::new();
    for _ in 0..8 {
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().without_tasks(),
        );
        let previous_count = descendants.len();
        let mut index = 0;
        while index < descendants.len() {
            let parent = descendants[index];
            for process in system.processes().values() {
                if process.parent() == Some(parent) && !descendants.contains(&process.pid()) {
                    descendants.push(process.pid());
                }
            }
            index += 1;
        }
        for pid in &descendants {
            if !identities
                .iter()
                .any(|entry: &ProcessIdentity| entry.pid == *pid)
            {
                if let Some(process) = system.process(*pid) {
                    identities.push(ProcessIdentity {
                        pid: *pid,
                        start: process.start_time(),
                        parent: process.parent(),
                    });
                }
            }
        }
        for identity in identities.iter().skip(1) {
            if process_in_group(identity.pid, pid) {
                continue;
            }
            signal_snapshot_process_with(
                identity,
                &identities,
                &mut system,
                rustix::process::Signal::STOP,
            );
        }
        if descendants.len() == previous_count {
            break;
        }
    }
    for identity in identities.iter().skip(1).rev() {
        if !process_in_group(identity.pid, pid) {
            signal_snapshot_process(identity, &identities, &mut system);
        }
    }
    let _ = kill_process_group(pid);
    for _ in 0..100 {
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().without_tasks(),
        );
        let mut index = 0;
        while index < descendants.len() {
            let parent = descendants[index];
            for process in system.processes().values() {
                if process.parent() == Some(parent) && !descendants.contains(&process.pid()) {
                    descendants.push(process.pid());
                    identities.push(ProcessIdentity {
                        pid: process.pid(),
                        start: process.start_time(),
                        parent: process.parent(),
                    });
                }
            }
            index += 1;
        }
        for identity in identities.iter().skip(1).rev() {
            if !process_in_group(identity.pid, pid) {
                signal_snapshot_process(identity, &identities, &mut system);
            }
        }
        let _ = kill_process_group(pid);
        if descendants.iter().all(|pid| {
            system.process(*pid).is_none_or(|process| {
                matches!(
                    process.status(),
                    sysinfo::ProcessStatus::Zombie | sysinfo::ProcessStatus::Dead
                )
            })
        }) {
            return true;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    false
}

fn process_in_group(process: sysinfo::Pid, group: u32) -> bool {
    rustix::process::Pid::from_raw(process.as_u32().cast_signed())
        .and_then(|pid| rustix::process::getpgid(Some(pid)).ok())
        .is_some_and(|pid| pid.as_raw_nonzero().get() as u32 == group)
}

// A descendant can start a new session and retain the write end after a group kill.
// Stop polling and close our read end before joining in that case.
fn join_killed_reader(reader: Option<PipeDrain>, grace: Duration) -> Vec<u8> {
    let Some(reader) = reader else {
        return Vec::new();
    };
    let deadline = Instant::now() + grace;
    while !reader.is_finished() && Instant::now() < deadline {
        std::thread::sleep(POLL_INTERVAL);
    }
    reader.stop.store(true, Ordering::Release);
    reader.handle.join().unwrap_or_default()
}

#[derive(Clone)]
struct ProcessIdentity {
    pid: sysinfo::Pid,
    start: u64,
    parent: Option<sysinfo::Pid>,
}

fn signal_snapshot_process(
    identity: &ProcessIdentity,
    snapshot: &[ProcessIdentity],
    system: &mut sysinfo::System,
) -> bool {
    signal_snapshot_process_with(identity, snapshot, system, rustix::process::Signal::KILL)
}

fn signal_snapshot_process_with(
    identity: &ProcessIdentity,
    snapshot: &[ProcessIdentity],
    system: &mut sysinfo::System,
    signal: rustix::process::Signal,
) -> bool {
    #[cfg(target_os = "linux")]
    let descriptor = {
        use rustix::process::{pidfd_open, Pid, PidfdFlags};
        let Some(pid) = Pid::from_raw(identity.pid.as_u32().cast_signed()) else {
            return false;
        };
        let Ok(fd) = pidfd_open(pid, PidfdFlags::empty()) else {
            return false;
        };
        fd
    };
    let pids: Vec<_> = snapshot.iter().map(|entry| entry.pid).collect();
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&pids),
        true,
        sysinfo::ProcessRefreshKind::nothing().without_tasks(),
    );
    if !identity_chain_matches(identity, snapshot, system) {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        rustix::process::pidfd_send_signal(&descriptor, signal).is_ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        // macOS has no pidfd. Exit and PID reuse can still occur after this check.
        rustix::process::Pid::from_raw(identity.pid.as_u32().cast_signed())
            .is_some_and(|pid| rustix::process::kill_process(pid, signal).is_ok())
    }
}

fn identity_chain_matches(
    identity: &ProcessIdentity,
    snapshot: &[ProcessIdentity],
    system: &sysinfo::System,
) -> bool {
    let mut current = identity;
    loop {
        if !system.process(current.pid).is_some_and(|process| {
            process.start_time() == current.start && process.parent() == current.parent
        }) {
            return false;
        }
        let Some(parent) = snapshot
            .iter()
            .find(|entry| Some(entry.pid) == current.parent)
        else {
            return true;
        };
        current = parent;
    }
}

fn child_exited_unreaped(child: &Child) -> std::io::Result<bool> {
    use rustix::process::{waitid, Pid, WaitId, WaitIdOptions};
    // WNOWAIT retains the leader PID and PGID until cleanup has signalled the group.
    waitid(
        WaitId::Pid(Pid::from_child(child)),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    )
    .map(|status| status.is_some())
    .map_err(Into::into)
}

impl ProcessSpawner for RealProcessSpawner {
    fn run(
        &self,
        spec: &ProcessSpec,
        cancel: &dyn CancelToken,
    ) -> Result<ProcessOutput, CoreError> {
        if cancel.is_cancelled() {
            return Err(CoreError::new(
                skill_studio_core::ErrorCode::Cancelled,
                "process cancelled",
            ));
        }
        let program = self.resolve_program(&spec.program);
        let probe_key = if spec.args == ["--version"] && spec.cwd.is_none() && spec.env.is_empty() {
            let path = if program.is_absolute() {
                Some(program.clone())
            } else {
                std::env::split_paths(&self.effective_path(spec))
                    .map(|dir| dir.join(&program))
                    .find(|path| is_executable_file(path))
            };
            path.and_then(|path| ProbeKey::read_with_path(&path, self.effective_path(spec)))
        } else {
            None
        };
        if let Some(output) = probe_key.as_ref().and_then(|key| {
            self.probes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(key)
        }) {
            return Ok(output);
        }
        let mut command = std::process::Command::new(&program);
        command.args(&spec.args);
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        for (key, value) in &spec.env {
            if value.is_empty() {
                command.env_remove(key);
            } else {
                command.env(key, value);
            }
        }
        if !self.search_dirs.is_empty() && !spec.env.iter().any(|(key, _)| key == "PATH") {
            command.env("PATH", self.child_path());
        }
        self.isolate_environment(&mut command);
        // No stdin: the MCP server's JSON-RPC stream must not reach a child,
        // and a CLI that asks a question then fails instead of waiting.
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let start = self.clock.monotonic();
        let mut child = spawn_retrying_busy(&mut command)
            .map_err(|e| CoreError::io(Path::new(&spec.program), e))?;

        // Drain both pipes concurrently with the poll loop below, not after
        // it: see `spawn_drain`'s doc comment.
        let stdout_reader: Option<PipeDrain> = child.stdout.take().map(spawn_drain::<ChildStdout>);
        let stderr_reader: Option<PipeDrain> = child.stderr.take().map(spawn_drain::<ChildStderr>);

        let deadline = Duration::from_millis(spec.timeout_ms);
        let exit_status = loop {
            if cancel.is_cancelled() {
                break Err(CoreError::new(
                    skill_studio_core::ErrorCode::Cancelled,
                    "process cancelled",
                ));
            }
            if self.clock.monotonic().saturating_sub(start) >= deadline {
                break Ok(None);
            }
            match child_exited_unreaped(&child) {
                Ok(true) => break Ok(Some(())),
                Ok(false) => std::thread::sleep(POLL_INTERVAL),
                Err(e) => break Err(CoreError::io(Path::new(&spec.program), e)),
            }
        };
        let tree_stopped = kill_process_tree(child.id());
        let _ = child.kill();
        let status = child
            .wait()
            .map_err(|e| CoreError::io(Path::new(&spec.program), e))?;
        if !tree_stopped {
            join_killed_reader(stdout_reader, self.reader_grace);
            join_killed_reader(stderr_reader, self.reader_grace);
            return Err(CoreError::new(
                skill_studio_core::ErrorCode::Io,
                "probe process tree did not stop",
            ));
        }
        match exit_status {
            Ok(Some(())) => {}
            stopped => {
                let stdout = join_killed_reader(stdout_reader, self.reader_grace);
                let stderr = join_killed_reader(stderr_reader, self.reader_grace);
                return stopped.map(|_| ProcessOutput {
                    status: None,
                    stdout: String::from_utf8_lossy(&stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&stderr).into_owned(),
                    timed_out: true,
                });
            }
        }
        let stdout = join_killed_reader(stdout_reader, self.reader_grace);
        let stderr = join_killed_reader(stderr_reader, self.reader_grace);
        let output = self.describe_failed_npx(
            spec,
            &program,
            ProcessOutput {
                status: status.code(),
                stdout: String::from_utf8_lossy(&stdout).into_owned(),
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
                timed_out: false,
            },
        );
        if let Some(key) = probe_key {
            // A binary replaced during the probe must not populate the old fingerprint.
            if key.is_current() {
                self.probes
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .put(key, &output);
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill_studio_core::ports::NeverCancel;

    fn write_script(path: &Path, body: &str) {
        crate::test_scripts::write_fake_executable(path, body);
    }

    #[test]
    fn cleanup_stops_group_before_collecting_or_allows_a_synchronized_late_fork() {
        use rustix::process::{waitid, Pid, WaitId, WaitIdOptions};
        use std::os::unix::process::CommandExt;
        let tmp = tempfile::tempdir().unwrap();
        let ready = tmp.path().join("ready");
        let release = tmp.path().join("release");
        let (send, recv) = std::sync::mpsc::channel();
        let forked = tmp.path().join("forked");
        let mut child = Command::new("python3")
            .args([
                "-c",
                r"import os, sys, signal
ready, release, forked = sys.argv[1:]
open(ready + '.tmp', 'w').close()
os.rename(ready + '.tmp', ready)
while not os.path.exists(release): pass
parent = os.getpid()
if os.fork() == 0:
    os.setsid()
    fd = os.open('/dev/null', os.O_RDWR)
    os.dup2(fd, 1)
    os.dup2(fd, 2)
    open(forked, 'w').close()
    os.kill(parent, signal.SIGSTOP)
    while True: pass
while True: pass
",
            ])
            .arg(&ready)
            .arg(&release)
            .arg(&forked)
            .process_group(0)
            .spawn()
            .unwrap();
        let observer = std::thread::spawn(move || {
            while !ready.exists() {
                std::thread::yield_now();
            }
            send.send(()).unwrap();
        });
        recv.recv().unwrap();
        observer.join().unwrap();
        let mut stopped = false;
        let cleaned = kill_process_tree_before_collect(child.id(), || {
            std::fs::write(release.with_extension("tmp"), b"release").unwrap();
            std::fs::rename(release.with_extension("tmp"), &release).unwrap();
            waitid(
                WaitId::Pid(Pid::from_child(&child)),
                WaitIdOptions::STOPPED | WaitIdOptions::NOWAIT,
            )
            .unwrap();
            stopped = !forked.exists();
        });
        child.wait().unwrap();
        assert!(stopped, "group was allowed to fork during collection");
        assert!(cleaned);
    }

    #[test]
    fn absolute_shim_uses_each_effective_path_or_reuses_the_previous_version() {
        let tmp = tempfile::tempdir().unwrap();
        let shim = tmp.path().join("shim");
        write_script(&shim, "exec backend --version");
        for (name, version) in [("first", "1.2.3"), ("second", "4.5.6")] {
            let bin = tmp.path().join(name);
            std::fs::create_dir(&bin).unwrap();
            write_script(&bin.join("backend"), &format!("echo {version}"));
        }
        for (name, version) in [("first", "1.2.3"), ("second", "4.5.6")] {
            let spawner = RealProcessSpawner::for_home(tmp.path(), vec![tmp.path().join(name)]);
            let spec = ProcessSpec {
                program: shim.display().to_string(),
                args: vec!["--version".into()],
                cwd: None,
                env: Vec::new(),
                timeout_ms: 30_000,
            };
            assert_eq!(
                spawner.run(&spec, &NeverCancel).unwrap().stdout.trim(),
                version,
                "shim reused the version from the previous PATH"
            );
        }
    }

    struct ProbeFixture {
        pid: PathBuf,
        release: PathBuf,
    }
    impl Drop for ProbeFixture {
        fn drop(&mut self) {
            let _ = std::fs::write(&self.release, b"release");
            if let Ok(pid) = std::fs::read_to_string(&self.pid)
                .unwrap_or_default()
                .trim()
                .parse::<u32>()
            {
                let _ = kill_process_tree(pid);
            }
        }
    }

    #[test]
    fn normal_exit_with_redirected_child_stops_the_group_or_leaves_a_probe_running() {
        let tmp = tempfile::tempdir().unwrap();
        let fixture = ProbeFixture {
            pid: tmp.path().join("pid"),
            release: tmp.path().join("release"),
        };
        let spec = ProcessSpec {
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                "sleep 600 >/dev/null 2>&1 & echo $! > \"$1\"; echo 1.2.3".into(),
                "probe".into(),
                fixture.pid.display().to_string(),
            ],
            cwd: None,
            env: Vec::new(),
            timeout_ms: 30_000,
        };
        let result = RealProcessSpawner::new().run(&spec, &NeverCancel);
        let pid = std::fs::read_to_string(&fixture.pid)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let alive = process_running(pid);
        drop(fixture);
        assert_eq!(result.unwrap().stdout.trim(), "1.2.3");
        assert!(!alive, "redirected child survived normal exit");
    }

    #[test]
    fn cleanup_rechecks_snapshot_identity_or_signals_a_reused_pid_or_parent() {
        use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
        let mut system = System::new().unwrap();
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().without_tasks(),
        );
        let mut child = Command::new("/bin/sleep").arg("600").spawn().unwrap();
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().without_tasks(),
        );
        let process = system.process(Pid::from_u32(child.id())).unwrap();
        let identity = ProcessIdentity {
            pid: process.pid(),
            start: process.start_time(),
            parent: process.parent(),
        };
        let mut reused = identity.clone();
        reused.start += 1;
        let signalled_reuse =
            signal_snapshot_process(&reused, std::slice::from_ref(&reused), &mut system);
        let survived_reuse = child.try_wait().unwrap().is_none();
        let mut reparented = identity;
        reparented.parent = None;
        let signalled_reparenting =
            signal_snapshot_process(&reparented, std::slice::from_ref(&reparented), &mut system);
        let survived_reparenting = child.try_wait().unwrap().is_none();
        let _ = child.kill();
        child.wait().unwrap();
        assert!(
            !signalled_reuse && survived_reuse,
            "signalled a PID with a changed start time"
        );
        assert!(
            !signalled_reparenting && survived_reparenting,
            "signalled a PID with a changed parent chain"
        );
    }

    #[test]
    fn exit_poll_keeps_leader_waitable_or_allows_pgid_reuse_before_cleanup() {
        let mut child = Command::new("/usr/bin/true").spawn().unwrap();
        while !child_exited_unreaped(&child).unwrap() {
            std::thread::yield_now();
        }
        let still_waitable = child_exited_unreaped(&child);
        child.wait().unwrap();
        assert!(still_waitable.unwrap());
    }

    #[test]
    fn cancelling_an_escaping_grandchild_stops_it_or_leaves_the_probe_running() {
        let tmp = tempfile::tempdir().unwrap();
        let fixture = ProbeFixture {
            pid: tmp.path().join("ready"),
            release: tmp.path().join("release"),
        };
        let script = r"import os, sys, time
if os.fork() == 0:
    os.setsid()
    with open(sys.argv[1] + '.tmp', 'w') as f:
        f.write(str(os.getpid()))
    os.rename(sys.argv[1] + '.tmp', sys.argv[1])
    while not os.path.exists(sys.argv[2]):
        time.sleep(0.01)
    os._exit(0)
while True:
    time.sleep(1)
";
        let spec = ProcessSpec {
            program: "/usr/bin/python3".into(),
            args: vec![
                "-c".into(),
                script.into(),
                fixture.pid.display().to_string(),
                fixture.release.display().to_string(),
            ],
            cwd: None,
            env: Vec::new(),
            timeout_ms: 30_000,
        };
        let spawner = RealProcessSpawner {
            reader_grace: Duration::ZERO,
            ..RealProcessSpawner::new()
        };
        let result = spawner.run(&spec, &CancelWhenPidReady(fixture.pid.clone()));
        let pid = std::fs::read_to_string(&fixture.pid)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let alive = process_running(pid);
        drop(fixture);
        assert!(
            matches!(result, Err(error) if error.code == skill_studio_core::ErrorCode::Cancelled)
        );
        assert!(!alive, "escaped descendant survived cancellation");
    }

    #[test]
    fn replacing_a_shell_launchers_backend_under_the_same_path_returns_the_new_version_or_reuses_stale_output(
    ) {
        let home = tempfile::tempdir().unwrap();
        let launcher = home.path().join("launcher");
        let backend = home.path().join("backend");
        write_script(&launcher, "exec backend --version");
        let spec = ProcessSpec {
            program: launcher.display().to_string(),
            args: vec!["--version".into()],
            cwd: None,
            env: Vec::new(),
            timeout_ms: 30_000,
        };
        for version in ["1.2.3", "4.5.6"] {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(&backend, format!("#!/bin/sh\necho {version}\n")).unwrap();
            std::fs::set_permissions(&backend, std::fs::Permissions::from_mode(0o755)).unwrap();
            let spawner =
                RealProcessSpawner::for_home(home.path(), vec![home.path().to_path_buf()]);
            assert_eq!(
                spawner.run(&spec, &NeverCancel).unwrap().stdout.trim(),
                version,
                "shell launcher reused the replaced backend's version"
            );
        }
    }

    #[test]
    fn replacing_a_python_launchers_backend_under_the_same_path_returns_the_new_version_or_reuses_stale_output(
    ) {
        let home = tempfile::tempdir().unwrap();
        let launcher = home.path().join("launcher");
        let backend = home.path().join("backend");
        std::fs::write(&launcher, "#!/usr/bin/python3\nimport subprocess\nsubprocess.run(['backend', '--version'], check=True)\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let spec = ProcessSpec {
            program: launcher.display().to_string(),
            args: vec!["--version".into()],
            cwd: None,
            env: Vec::new(),
            timeout_ms: 30_000,
        };
        for version in ["1.2.3", "4.5.6"] {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(&backend, format!("#!/bin/sh\necho {version}\n")).unwrap();
            std::fs::set_permissions(&backend, std::fs::Permissions::from_mode(0o755)).unwrap();
            let spawner =
                RealProcessSpawner::for_home(home.path(), vec![home.path().to_path_buf()]);
            assert_eq!(
                spawner.run(&spec, &NeverCancel).unwrap().stdout.trim(),
                version,
                "shell launcher reused the replaced backend's version"
            );
        }
    }

    fn write_program(path: &Path) {
        let source = path.with_extension("c");
        std::fs::write(
            &source,
            r#"#include <stdio.h>
int main(int argc, char **argv) {
    char calls[4096];
    snprintf(calls, sizeof(calls), "%s.calls", argv[0]);
    FILE *f = fopen(calls, "a");
    if (!f) return 1;
    fputs("called\n", f); fclose(f);
    puts("1.2.3"); return 0;
}
"#,
        )
        .unwrap();
        assert!(Command::new("/usr/bin/cc")
            .arg(&source)
            .arg("-o")
            .arg(path)
            .status()
            .unwrap()
            .success());
    }

    #[test]
    fn unchanged_version_probe_runs_once_or_repeats_the_process() {
        let tmp = tempfile::tempdir().unwrap();
        let program = tmp.path().join("probe");
        write_program(&program);
        let spec = ProcessSpec {
            program: program.display().to_string(),
            args: vec!["--version".into()],
            cwd: None,
            env: Vec::new(),
            timeout_ms: 30_000,
        };
        let spawner = RealProcessSpawner::new();
        let first = spawner.run(&spec, &NeverCancel).unwrap();
        let second = spawner.run(&spec, &NeverCancel).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("probe.calls"))
                .unwrap()
                .lines()
                .count(),
            1,
            "unchanged version probe ran twice"
        );
    }

    #[test]
    fn version_cache_survives_a_new_spawner_or_probes_again_on_the_next_launch() {
        let tmp = tempfile::tempdir().unwrap();
        let program = tmp.path().join("probe");
        write_program(&program);
        let spec = ProcessSpec {
            program: program.display().to_string(),
            args: vec!["--version".into()],
            cwd: None,
            env: Vec::new(),
            timeout_ms: 30_000,
        };
        let cache = tmp.path().join("app-cache/version-probes.json");
        let first = RealProcessSpawner::new()
            .with_probe_cache(cache.clone())
            .run(&spec, &NeverCancel)
            .unwrap();
        let second = RealProcessSpawner::new()
            .with_probe_cache(cache)
            .run(&spec, &NeverCancel)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("probe.calls"))
                .unwrap()
                .lines()
                .count(),
            1,
            "new spawner did not reuse the persisted probe"
        );
        let ordinary = ProcessSpec {
            args: Vec::new(),
            ..spec
        };
        let spawner = RealProcessSpawner::new();
        spawner.run(&ordinary, &NeverCancel).unwrap();
        spawner.run(&ordinary, &NeverCancel).unwrap();
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("probe.calls"))
                .unwrap()
                .lines()
                .count(),
            3,
            "ordinary commands must run every time"
        );
    }

    #[test]
    fn cancelling_detection_reaches_the_version_probe_and_returns_cancelled_or_reports_success() {
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Cancellation(Arc<AtomicBool>);
        impl CancelToken for Cancellation {
            fn is_cancelled(&self) -> bool {
                self.0.load(Ordering::SeqCst)
            }
        }
        struct CancelProbe(Arc<AtomicBool>);
        impl ProcessSpawner for CancelProbe {
            fn run(
                &self,
                _: &ProcessSpec,
                cancel: &dyn CancelToken,
            ) -> Result<ProcessOutput, CoreError> {
                self.0.store(true, Ordering::SeqCst);
                assert!(
                    cancel.is_cancelled(),
                    "detection replaced its caller's cancellation token"
                );
                Err(CoreError::new(
                    skill_studio_core::ErrorCode::Cancelled,
                    "cancelled",
                ))
            }
        }
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        write_script(&bin.join("claude"), "echo 1.2.3");
        let flag = Arc::new(AtomicBool::new(false));
        let scope = skill_studio_core::RuntimeScope::fixture(&home);
        let mut ports = crate::default_ports(
            tmp.path().join("leases"),
            Arc::new(skill_studio_core::harness::HarnessCatalog::builtin()),
        );
        ports.tools = Some(Arc::new(crate::PathToolLookup::with_search_dirs(vec![bin])));
        ports.spawner = Some(Arc::new(CancelProbe(flag.clone())));
        let rt = skill_studio_core::ports::Runtime::new(&scope, ports).unwrap();
        let context = skill_studio_core::ports::OpContext::with_cancel(
            skill_studio_core::identity::CorrelationId("cancel".into()),
            Arc::new(Cancellation(flag)),
        );
        let result = skill_studio_core::ops::harnesses(
            &rt,
            &context,
            &skill_studio_core::dto::HarnessesRequest {},
        );
        assert!(
            matches!(result, Err(ref error) if error.code == skill_studio_core::ErrorCode::Cancelled),
            "{result:?}"
        );
    }

    struct CancelWhenPidReady(PathBuf);
    impl CancelToken for CancelWhenPidReady {
        fn is_cancelled(&self) -> bool {
            std::fs::read_to_string(&self.0).is_ok_and(|value| value.trim().parse::<u32>().is_ok())
        }
    }

    #[test]
    fn escaped_child_readiness_waits_for_a_parseable_pid_or_cancels_before_pid_publication() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("ready");
        let cancel = CancelWhenPidReady(path.clone());
        std::fs::write(&path, "").unwrap();
        assert!(!cancel.is_cancelled());
        std::fs::write(&path, "pending").unwrap();
        assert!(!cancel.is_cancelled());
        std::fs::write(&path, "123").unwrap();
        assert!(cancel.is_cancelled());
    }

    struct CancelWhenReady(PathBuf);
    impl CancelToken for CancelWhenReady {
        fn is_cancelled(&self) -> bool {
            std::fs::read_to_string(&self.0).is_ok_and(|value| !value.trim().is_empty())
        }
    }

    #[test]
    fn cancel_after_child_readiness_stops_the_probe_or_returns_a_normal_exit() {
        let tmp = tempfile::tempdir().unwrap();
        let ready = tmp.path().join("ready");
        let spec = ProcessSpec {
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                "echo ready > \"$1\"; exec /bin/sleep 2".into(),
                "probe".into(),
                ready.display().to_string(),
            ],
            cwd: None,
            env: Vec::new(),
            timeout_ms: 30_000,
        };
        let result = RealProcessSpawner::new().run(&spec, &CancelWhenReady(ready));
        assert!(
            matches!(result, Err(ref e) if e.code == skill_studio_core::ErrorCode::Cancelled),
            "{result:?}"
        );
    }

    #[test]
    fn normal_exit_releases_inherited_pipes_before_the_test_releases_the_descendant_or_hangs() {
        let tmp = tempfile::tempdir().unwrap();
        let ready = tmp.path().join("ready");
        let release = tmp.path().join("release");
        let spec = ProcessSpec {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "(echo ready > \"$1\"; while [ ! -f \"$2\" ]; do /bin/sleep 0.01; done) & while [ ! -f \"$1\" ]; do /bin/sleep 0.01; done; echo parent-done".into(), "probe".into(), ready.display().to_string(), release.display().to_string()],
            cwd: None, env: Vec::new(), timeout_ms: 30_000,
        };
        let (send, recv) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            send.send(
                RealProcessSpawner {
                    reader_grace: Duration::ZERO,
                    ..RealProcessSpawner::new()
                }
                .run(&spec, &NeverCancel),
            )
        });
        let result = recv.recv_timeout(Duration::from_secs(30));
        std::fs::write(release, b"release").unwrap();
        worker.join().unwrap().ok();
        let output = result
            .expect("normal exit waited for the descendant's release")
            .unwrap();
        assert_eq!(output.status, Some(0));
        assert!(!output.timed_out);
        assert!(output.stdout.contains("parent-done"), "{output:?}");
    }

    /// `a_busy_executable_is_retried_until_free_or_the_busy_error_surfaces_after_the_cap`:
    /// on Linux a just-written script can be "Text file busy" for a few
    /// milliseconds. Fails if the first ETXTBSY is returned to the caller
    /// instead of retried, or if a program that stays busy is retried
    /// without bound.
    #[test]
    fn a_busy_executable_is_retried_until_free_or_the_busy_error_surfaces_after_the_cap() {
        let busy = || std::io::Error::from(std::io::ErrorKind::ExecutableFileBusy);

        let mut calls = 0;
        let result = retry_when_busy(|| {
            calls += 1;
            if calls < 3 {
                Err(busy())
            } else {
                Ok(calls)
            }
        });
        assert_eq!(result.unwrap(), 3, "two busy attempts must be retried");

        let mut calls = 0;
        let error = retry_when_busy::<()>(|| {
            calls += 1;
            Err(busy())
        })
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::ExecutableFileBusy);
        assert_eq!(calls, BUSY_SPAWN_ATTEMPTS, "retries must stop at the cap");

        let mut calls = 0;
        let error = retry_when_busy::<()>(|| {
            calls += 1;
            Err(std::io::Error::from(std::io::ErrorKind::NotFound))
        })
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(calls, 1, "any other spawn error must not be retried");
    }

    fn run_fake_npx(dir: &Path) -> ProcessOutput {
        let spec = ProcessSpec {
            program: "npx".into(),
            args: vec!["skills".into(), "update".into(), "foo".into()],
            cwd: None,
            env: Vec::new(),
            timeout_ms: 10_000,
        };
        RealProcessSpawner::with_search_path(vec![dir.to_path_buf()])
            .run(&spec, &NeverCancel)
            .unwrap()
    }

    /// `a_failed_npx_names_the_program_argv_and_node_it_ran_or_names_the_missing_part`:
    /// a user must be able to see which `npx` and `node` ran. Fails if the
    /// failure output lacks the resolved program, the argv, or the node path.
    #[test]
    fn a_failed_npx_names_the_program_argv_and_node_it_ran_or_names_the_missing_part() {
        let tmp = tempfile::tempdir().unwrap();
        write_script(&tmp.path().join("npx"), "echo boom >&2; exit 1");
        write_script(&tmp.path().join("node"), "echo v22.0.0");

        let output = run_fake_npx(tmp.path());

        let expected = format!(
            "Ran: {0}/npx skills update foo (node: {0}/node)",
            tmp.path().display()
        );
        assert!(
            output.stderr.contains("boom") && output.stderr.contains(&expected),
            "stderr lacks `{expected}`: {:?}",
            output.stderr
        );
    }

    /// `a_node_that_cannot_load_its_library_is_reported_as_broken_or_shows_the_raw_npx_text`:
    /// Homebrew's `node` fails with dyld "Library not loaded" after a
    /// dependency upgrade. Fails if the output keeps npx's raw text instead
    /// of naming the broken Node.
    #[test]
    fn a_node_that_cannot_load_its_library_is_reported_as_broken_or_shows_the_raw_npx_text() {
        let tmp = tempfile::tempdir().unwrap();
        write_script(&tmp.path().join("npx"), "echo 'env: node: bad' >&2; exit 1");
        write_script(
            &tmp.path().join("node"),
            "echo 'dyld[1]: Library not loaded: libsimdjson.dylib' >&2; exit 1",
        );

        let output = run_fake_npx(tmp.path());

        let broken = format!(
            "The Node at {}/node is broken: dyld[1]: Library not loaded",
            tmp.path().display()
        );
        assert!(
            output.stderr.contains(&broken),
            "stderr does not report the broken Node: {:?}",
            output.stderr
        );
    }

    #[test]
    fn run_captures_stdout_and_exit_status_or_names_the_missing_field() {
        let spawner = RealProcessSpawner::new();
        let spec = ProcessSpec {
            program: "echo".into(),
            args: vec!["hello".into()],
            cwd: None,
            env: Vec::new(),
            timeout_ms: 30_000,
        };
        let output = spawner.run(&spec, &NeverCancel).unwrap();
        assert_eq!(output.status, Some(0), "echo did not exit 0");
        assert_eq!(output.stdout.trim(), "hello");
        assert!(!output.timed_out);
    }

    #[test]
    fn a_hung_version_probe_times_out_and_is_killed_or_names_the_probe_that_hangs() {
        let tmp = tempfile::tempdir().unwrap();
        let pid_file = tmp.path().join("pid");
        let spawner = RealProcessSpawner {
            clock: Arc::new(ReadyClock(pid_file.clone())),
            ..RealProcessSpawner::new()
        };
        let spec = ProcessSpec {
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                format!("echo $$ > '{}'; exec sleep 30", pid_file.display()),
            ],
            cwd: None,
            env: Vec::new(),
            timeout_ms: 30_000,
        };

        let output = spawner.run(&spec, &NeverCancel).unwrap();

        assert!(
            output.timed_out,
            "a probe past its deadline must report timed_out, got {output:?}"
        );
        let pid = std::fs::read_to_string(&pid_file).unwrap();
        let pid = pid.trim();
        assert!(
            !pid.is_empty(),
            "the child never recorded its pid, so the kill cannot be checked"
        );
        let still_alive = std::process::Command::new("/bin/kill")
            .args(["-0", pid])
            .status()
            .unwrap()
            .success();
        assert!(
            !still_alive,
            "the hung probe (pid {pid}) is still running after run() returned - it was abandoned, not killed"
        );
    }

    #[test]
    fn run_reports_a_spawn_error_for_a_missing_binary_rather_than_panicking() {
        let spawner = RealProcessSpawner::new();
        let spec = ProcessSpec {
            program: "definitely-not-a-real-skill-studio-binary".into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            timeout_ms: 2_000,
        };
        let err = spawner
            .run(&spec, &NeverCancel)
            .expect_err("a nonexistent program must not spawn");
        assert_eq!(err.code, skill_studio_core::ErrorCode::Io);
    }

    /// `a_packaged_apps_minimal_process_path_still_resolves_a_bare_npx_and_the_node_its_shebang_needs_or_names_which_lookup_starved`:
    /// a fake `npx` (`#!/bin/sh` that `exec`s `/usr/bin/env node`) and a
    /// fake `node`, both only in a temp dir the *process's own* `PATH` does
    /// not contain, mimic a packaged desktop app launched from Finder under
    /// `launchd`'s minimal `PATH`: without `with_search_path`, `Command::new
    /// ("npx")` can't find the bare name, and even given `npx`'s absolute
    /// path directly, its own `env node` step still can't find `node` on
    /// the child's `PATH`. Fails (red) on `RealProcessSpawner::new()` -
    /// spawn error or empty stdout - and on `with_search_path` without the
    /// child `PATH` env write.
    #[test]
    fn a_packaged_apps_minimal_process_path_still_resolves_a_bare_npx_and_the_node_its_shebang_needs_or_names_which_lookup_starved(
    ) {
        let tmp = tempfile::tempdir().unwrap();
        write_script(&tmp.path().join("npx"), "exec /usr/bin/env node");
        write_script(&tmp.path().join("node"), "echo fake-node-ok");
        let inherited_path = std::env::var_os("PATH").unwrap_or_default();
        assert!(
            !std::env::split_paths(&inherited_path).any(|dir| dir == tmp.path()),
            "test setup bug: the fake tool dir must not already be on this process's own PATH"
        );

        let spawner = RealProcessSpawner::with_search_path(vec![tmp.path().to_path_buf()]);
        let spec = ProcessSpec {
            program: "npx".into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            // Generous: this only needs to outlast a trivial `sh`/`env`
            // spawn, but a full `cargo test --workspace` run schedules this
            // alongside other tests that spawn and sleep real child
            // processes, and 2s was tight enough under that load to time
            // out this fake `npx` before it ever ran.
            timeout_ms: 30_000,
        };

        let output = spawner.run(&spec, &NeverCancel).unwrap();

        assert!(!output.timed_out, "the fake npx never ran: {output:?}");
        assert_eq!(
            output.stdout.trim(),
            "fake-node-ok",
            "npx's own `env node` step could not find node on the child's PATH: {output:?}"
        );
    }

    /// `a_child_writing_past_the_pipe_buffer_still_finishes_by_its_deadline_or_names_the_hang`:
    /// `sh -c 'yes x | head -c 200000'` writes 200 KB to stdout, more than a
    /// pipe's buffer (about 64 KB). Draining stdout only starts after
    /// `try_wait` first sees the child exit; if nothing reads the pipe while
    /// polling, the write blocks, the child never exits, and `run` always
    /// hits the deadline instead of the child's real completion. Fails (red)
    /// on the old poll-then-read order: `timed_out` comes back `true` and
    /// `stdout` is empty or truncated instead of the full 200000 bytes.
    #[test]
    fn a_child_writing_past_the_pipe_buffer_still_finishes_by_its_deadline_or_names_the_hang() {
        let spawner = RealProcessSpawner::new();
        let spec = ProcessSpec {
            program: "sh".into(),
            args: vec!["-c".into(), "yes x | head -c 200000".into()],
            cwd: None,
            env: Vec::new(),
            timeout_ms: 30_000,
        };

        let output = spawner.run(&spec, &NeverCancel).unwrap();

        assert!(
            !output.timed_out,
            "a child writing past the pipe buffer was reported as hung: {output:?}"
        );
        assert_eq!(
            output.stdout.len(),
            200_000,
            "expected the full 200000 bytes the child wrote, got {}",
            output.stdout.len()
        );
    }

    struct ReadyClock(PathBuf);
    impl skill_studio_core::ports::Clock for ReadyClock {
        fn now(&self) -> chrono::DateTime<chrono::Utc> {
            chrono::Utc::now()
        }
        fn monotonic(&self) -> Duration {
            if std::fs::read_to_string(&self.0).is_ok_and(|pid| pid.trim().parse::<u32>().is_ok()) {
                Duration::from_secs(31)
            } else {
                Duration::ZERO
            }
        }
    }

    fn process_running(pid: u32) -> bool {
        let pid = sysinfo::Pid::from_u32(pid);
        let mut system = sysinfo::System::new().expect("process inspector");
        system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
        system.process(pid).is_some_and(|p| {
            !matches!(
                p.status(),
                sysinfo::ProcessStatus::Zombie | sysinfo::ProcessStatus::Dead
            )
        })
    }

    #[test]
    fn a_timed_out_childs_grandchild_that_inherited_the_pipe_is_still_killed_or_names_the_orphan_left_running(
    ) {
        let tmp = tempfile::tempdir().unwrap();
        let pid_file = tmp.path().join("grandchild_pid");
        let release = tmp.path().join("release");
        let spawner = RealProcessSpawner {
            clock: Arc::new(ReadyClock(pid_file.clone())),
            ..RealProcessSpawner::new()
        };
        let spec = ProcessSpec {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), r#"/bin/sh -c 'echo $$ > "$1"; while [ ! -f "$2" ]; do /bin/sleep 0.01; done' grandchild "$1" "$2" & wait"#.into(), "probe".into(), pid_file.display().to_string(), release.display().to_string()],
            cwd: None, env: Vec::new(), timeout_ms: 30_000,
        };
        let fixture = ProbeFixture {
            pid: pid_file.clone(),
            release,
        };
        let (send, recv) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || send.send(spawner.run(&spec, &NeverCancel)));
        let result = recv.recv_timeout(Duration::from_secs(30));
        let pid = std::fs::read_to_string(&pid_file)
            .unwrap_or_default()
            .trim()
            .parse::<u32>();
        let alive = pid.as_ref().is_ok_and(|pid| process_running(*pid));
        drop(fixture);
        worker.join().unwrap().ok();
        let output = result
            .expect("logical deadline waited for the test's release")
            .unwrap();
        assert!(output.timed_out, "{output:?}");
        assert!(pid.is_ok(), "grandchild did not signal readiness");
        assert!(!alive, "the grandchild survived the group kill");
    }
}
