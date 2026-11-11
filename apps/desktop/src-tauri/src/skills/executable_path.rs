use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn pick_executable_line(
    stdout: &str,
    is_executable: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let last = stdout
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())?;
    if !last.starts_with('/') {
        return None;
    }
    let path = PathBuf::from(last);
    if is_executable(&path) {
        Some(path)
    } else {
        None
    }
}

pub(crate) fn is_executable_file(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match fs::metadata(path) {
            Ok(meta) => meta.is_file() && meta.permissions().mode() & 0o111 != 0,
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        fs::metadata(path).is_ok_and(|m| m.is_file())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn always_executable(_: &Path) -> bool {
        true
    }

    fn never_executable(_: &Path) -> bool {
        false
    }

    #[test]
    fn pick_executable_line_skips_startup_noise() {
        let stdout = "Welcome to zsh\nLoading dotfiles...\n/usr/local/bin/claude\n";
        assert_eq!(
            pick_executable_line(stdout, always_executable),
            Some(PathBuf::from("/usr/local/bin/claude"))
        );
    }

    #[test]
    fn pick_executable_line_rejects_an_alias_line() {
        let stdout = "alias claude=/usr/local/bin/claude-old\n";
        assert_eq!(pick_executable_line(stdout, always_executable), None);
    }

    #[test]
    fn pick_executable_line_rejects_a_shell_function() {
        let stdout = "claude () {\n  echo hi\n}\n";
        assert_eq!(pick_executable_line(stdout, always_executable), None);
    }

    #[test]
    fn pick_executable_line_rejects_a_relative_name() {
        let stdout = "claude\n";
        assert_eq!(pick_executable_line(stdout, always_executable), None);
    }

    #[test]
    fn pick_executable_line_rejects_a_non_executable_path() {
        let stdout = "/usr/local/bin/claude\n";
        assert_eq!(pick_executable_line(stdout, never_executable), None);
    }

    #[test]
    fn pick_executable_line_accepts_a_path_with_spaces() {
        let stdout = "/opt/my app/bin/claude\n";
        assert_eq!(
            pick_executable_line(stdout, always_executable),
            Some(PathBuf::from("/opt/my app/bin/claude"))
        );
    }
}
