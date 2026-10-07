use std::path::Path;

/// Writes an executable at `path` that runs the shell `body`, without
/// writing a new executable file: `path` is a hard link to one runner script
/// made once per test process, and `body` sits beside it as `<path>.body`
/// for the runner to source. On macOS hosts that scan new executables,
/// running a freshly written script can stall for 10 s to minutes, which
/// trips process timeouts in unrelated tests; a new link to an
/// already-run file does not. Falls back to a plain script if the link
/// fails (a temp dir on another filesystem).
pub(crate) fn write_fake_executable(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    static RUNNER: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    let runner = RUNNER.get_or_init(|| {
        let runner = tempfile::tempdir().unwrap().keep().join("runner");
        std::fs::write(&runner, "#!/bin/sh\n. \"$0.body\"\n").unwrap();
        std::fs::set_permissions(&runner, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Pay the first-run stall here, outside any test's process deadline.
        let mut warm_body = runner.as_os_str().to_owned();
        warm_body.push(".body");
        std::fs::write(warm_body, ":\n").unwrap();
        assert!(std::process::Command::new(&runner)
            .status()
            .unwrap()
            .success());
        runner
    });
    if std::fs::hard_link(runner, path).is_ok() {
        let mut body_path = path.as_os_str().to_owned();
        body_path.push(".body");
        std::fs::write(body_path, body).unwrap();
    } else {
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
