// ============================================================================
// Skills Module - skill_plugin_lifecycle
// Claude Code plugin actions: disable, enable, update, and uninstall a plugin
// through the scriptable `claude plugin` CLI. Every skill a plugin ships
// moves together, since Claude Code tracks the switch per plugin, not per
// skill. Codex has no plugin CLI - plugins there are managed with
// `/plugins` inside a Codex session, so this module only ever runs `claude`.
// ============================================================================

use std::path::Path;

use super::skill_process::CommandRunner;

const CLAUDE_CLI: &str = "claude";

/// `claude plugin disable|enable <plugin_id> -s user`.
pub fn plugin_set_enabled_args(plugin_id: &str, enabled: bool) -> Vec<String> {
    vec![
        "plugin".to_string(),
        (if enabled { "enable" } else { "disable" }).to_string(),
        plugin_id.to_string(),
        "-s".to_string(),
        "user".to_string(),
    ]
}

/// `claude plugin uninstall <plugin_id> -s user -y`.
pub fn plugin_uninstall_args(plugin_id: &str) -> Vec<String> {
    vec![
        "plugin".to_string(),
        "uninstall".to_string(),
        plugin_id.to_string(),
        "-s".to_string(),
        "user".to_string(),
        "-y".to_string(),
    ]
}

/// `claude plugin update <plugin_id> -s <scope> --json`. Never passes `-y` or
/// `--accept-command`: running a command a marketplace declares is the
/// person's decision, made in a terminal.
pub fn plugin_update_args(plugin_id: &str, scope: &str) -> Vec<String> {
    vec![
        "plugin".to_string(),
        "update".to_string(),
        plugin_id.to_string(),
        "-s".to_string(),
        scope.to_string(),
        "--json".to_string(),
    ]
}

/// True for `<plugin>@<marketplace>`, both halves non-empty and built only
/// from the characters a plugin cache directory name allows (letters,
/// digits, `.`, `_`, `-`) - the same id core's `PluginSourceDto` and the
/// desktop's `PluginInfo::id` build as `"{plugin}@{marketplace}"`.
fn is_valid_plugin_id(plugin_id: &str) -> bool {
    let Some((plugin, marketplace)) = plugin_id.split_once('@') else {
        return false;
    };
    let is_id_part = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    };
    is_id_part(plugin) && is_id_part(marketplace)
}

/// Rewrites a spawn failure for a missing `claude` binary into a message the
/// user can act on. `CommandRunner::run` reports every failure as a plain
/// string, so this matches on the OS's "no such program" wording rather than
/// an error kind.
fn friendly_error(message: String) -> String {
    if message.contains("No such file or directory") || message.contains("cannot find the file") {
        "Claude Code CLI (`claude`) was not found on PATH.".to_string()
    } else {
        message
    }
}

/// Rewrites the CLI's refusal to run a marketplace-declared command without
/// confirmation (`-y`, or a TTY) into the next step for the person.
fn update_error(plugin_id: &str, message: String) -> String {
    let lower = message.to_lowercase();
    // The wording of `claude plugin update --help` for `-y` and
    // `--accept-command`; bare "confirm" or "tty" also match unrelated errors.
    let needs_confirmation =
        lower.contains("--yes") || lower.contains("pass -y") || lower.contains("--accept-command");
    if needs_confirmation {
        format!(
            "This update runs a command from the plugin's marketplace that needs your OK. Run `claude plugin update {plugin_id}` in a terminal to review it."
        )
    } else {
        friendly_error(message)
    }
}

/// The Tauri commands in this module refuse to run against anything but
/// Claude Code: Codex has no plugin CLI, and plugins there are managed with
/// `/plugins` inside a Codex session instead.
pub fn require_claude_code_harness(harness: &str) -> Result<(), String> {
    if harness == "Claude Code" {
        Ok(())
    } else {
        Err("Only Claude Code plugins can be managed from Skill Studio.".to_string())
    }
}

fn require_valid_plugin_id(plugin_id: &str) -> Result<(), String> {
    if is_valid_plugin_id(plugin_id) {
        Ok(())
    } else {
        Err(format!(
            "Not a plugin id in \"<plugin>@<marketplace>\" form: {plugin_id}"
        ))
    }
}

/// Runs `claude plugin disable|enable` for one plugin.
pub fn set_plugin_enabled_with(
    runner: &dyn CommandRunner,
    plugin_id: &str,
    enabled: bool,
) -> Result<(), String> {
    require_valid_plugin_id(plugin_id)?;
    runner
        .run(
            CLAUDE_CLI,
            &plugin_set_enabled_args(plugin_id, enabled),
            None::<&Path>,
        )
        .map_err(friendly_error)
}

/// Runs `claude plugin uninstall` for one plugin.
pub fn uninstall_plugin_with(runner: &dyn CommandRunner, plugin_id: &str) -> Result<(), String> {
    require_valid_plugin_id(plugin_id)?;
    runner
        .run(CLAUDE_CLI, &plugin_uninstall_args(plugin_id), None::<&Path>)
        .map_err(friendly_error)
}

/// Reads the `--json` result line of `claude plugin update`: its
/// `updateOutcome` (`updated`, `up_to_date`, ...). An `outcome` of `error`
/// becomes the CLI's own message. Output that is not JSON counts as `updated`,
/// since a zero exit code already said the command worked.
fn parse_update_outcome(stdout: &[u8]) -> Result<String, String> {
    let text = String::from_utf8_lossy(stdout);
    let parsed = text
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok());
    let Some(value) = parsed else {
        return Ok("updated".to_string());
    };
    if value.get("outcome").and_then(|v| v.as_str()) == Some("error") {
        let message = value
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("The plugin update failed.");
        return Err(message.to_string());
    }
    Ok(value
        .get("updateOutcome")
        .and_then(|v| v.as_str())
        .unwrap_or("updated")
        .to_string())
}

/// Runs `claude plugin update` for one install of a plugin and returns the
/// CLI's `updateOutcome`. `scope` is the install's own scope from
/// `installed_plugins.json`; a project or local install runs in its project
/// folder, where Claude Code resolves it.
pub fn update_plugin_with(
    runner: &dyn CommandRunner,
    plugin_id: &str,
    scope: &str,
    project_path: Option<&Path>,
) -> Result<String, String> {
    require_valid_plugin_id(plugin_id)?;
    match scope {
        "user" => {}
        "project" | "local" => {
            if project_path.is_none() {
                return Err(format!(
                    "The {scope} install of {plugin_id} has no project folder to update from."
                ));
            }
        }
        "managed" => return Err("This plugin is managed by your organization.".to_string()),
        other => return Err(format!("Unknown plugin scope: {other}")),
    }
    let cwd = if scope == "user" { None } else { project_path };
    if let Some(dir) = cwd.filter(|dir| !dir.is_dir()) {
        return Err(format!(
            "The project folder {} no longer exists.",
            dir.display()
        ));
    }
    let stdout = runner
        .run_output(CLAUDE_CLI, &plugin_update_args(plugin_id, scope), cwd)
        .map_err(|message| update_error(plugin_id, message))?;
    parse_update_outcome(&stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeRunner {
        calls: Mutex<Vec<(String, Vec<String>)>>,
        cwds: Mutex<Vec<Option<PathBuf>>>,
        fail: Option<String>,
        stdout: String,
    }

    impl CommandRunner for FakeRunner {
        fn run(&self, program: &str, args: &[String], cwd: Option<&Path>) -> Result<(), String> {
            self.cwds.lock().unwrap().push(cwd.map(Path::to_path_buf));
            self.calls
                .lock()
                .unwrap()
                .push((program.to_string(), args.to_vec()));
            match &self.fail {
                Some(err) => Err(err.clone()),
                None => Ok(()),
            }
        }

        fn run_output(
            &self,
            program: &str,
            args: &[String],
            cwd: Option<&Path>,
        ) -> Result<Vec<u8>, String> {
            self.run(program, args, cwd)?;
            Ok(self.stdout.clone().into_bytes())
        }
    }

    #[test]
    fn disable_builds_the_claude_disable_command() {
        let runner = FakeRunner::default();
        set_plugin_enabled_with(&runner, "codex@anthropics", false).unwrap();
        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "claude");
        assert_eq!(
            calls[0].1,
            vec!["plugin", "disable", "codex@anthropics", "-s", "user"]
        );
    }

    #[test]
    fn enable_builds_the_claude_enable_command() {
        let runner = FakeRunner::default();
        set_plugin_enabled_with(&runner, "codex@anthropics", true).unwrap();
        let calls = runner.calls.lock().unwrap();
        assert_eq!(
            calls[0].1,
            vec!["plugin", "enable", "codex@anthropics", "-s", "user"]
        );
    }

    #[test]
    fn uninstall_builds_the_claude_uninstall_command() {
        let runner = FakeRunner::default();
        uninstall_plugin_with(&runner, "codex@anthropics").unwrap();
        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls[0].0, "claude");
        assert_eq!(
            calls[0].1,
            vec![
                "plugin",
                "uninstall",
                "codex@anthropics",
                "-s",
                "user",
                "-y"
            ]
        );
    }

    #[test]
    fn invalid_plugin_id_is_rejected_without_running_anything() {
        let runner = FakeRunner::default();
        let err = set_plugin_enabled_with(&runner, "not-a-plugin-id", true).unwrap_err();
        assert!(err.contains("plugin"));
        assert!(runner.calls.lock().unwrap().is_empty());

        let err = uninstall_plugin_with(&runner, "codex@").unwrap_err();
        assert!(err.contains("plugin"));
        assert!(runner.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn non_claude_code_harness_is_rejected() {
        assert!(require_claude_code_harness("Claude Code").is_ok());
        let err = require_claude_code_harness("Codex").unwrap_err();
        assert!(err.contains("Claude Code"));
    }

    #[test]
    fn user_update_runs_claude_plugin_update_in_user_scope_without_yes() {
        let runner = FakeRunner::default();
        update_plugin_with(&runner, "sentry@claude-plugins-official", "user", None).unwrap();
        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls[0].0, "claude");
        assert_eq!(
            calls[0].1,
            vec![
                "plugin",
                "update",
                "sentry@claude-plugins-official",
                "-s",
                "user",
                "--json"
            ]
        );
        assert!(!calls[0].1.iter().any(|a| a == "-y" || a == "--yes"));
        assert_eq!(runner.cwds.lock().unwrap()[0], None);
    }

    #[test]
    fn project_update_runs_in_the_install_project_folder() {
        let runner = FakeRunner::default();
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path();
        update_plugin_with(
            &runner,
            "plugin-dev@claude-plugins-official",
            "project",
            Some(project),
        )
        .unwrap();
        assert_eq!(
            runner.calls.lock().unwrap()[0].1,
            vec![
                "plugin",
                "update",
                "plugin-dev@claude-plugins-official",
                "-s",
                "project",
                "--json"
            ]
        );
        assert_eq!(runner.cwds.lock().unwrap()[0].as_deref(), Some(project));
    }

    #[test]
    fn project_update_with_a_missing_folder_names_the_folder_and_runs_nothing() {
        let runner = FakeRunner::default();
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("deleted");
        let err =
            update_plugin_with(&runner, "codex@anthropics", "local", Some(&gone)).unwrap_err();
        assert_eq!(
            err,
            format!("The project folder {} no longer exists.", gone.display())
        );
        assert!(runner.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn update_returns_the_outcome_the_cli_reports() {
        let runner = FakeRunner {
            stdout: r#"{"outcome":"success","updateOutcome":"up_to_date","oldVersion":"1.4.0","newVersion":"1.4.0"}"#
                .to_string(),
            ..Default::default()
        };
        let outcome = update_plugin_with(&runner, "sentry@anthropics", "user", None).unwrap();
        assert_eq!(outcome, "up_to_date");

        let runner = FakeRunner {
            stdout: r#"{"outcome":"success","updateOutcome":"updated"}"#.to_string(),
            ..Default::default()
        };
        assert_eq!(
            update_plugin_with(&runner, "sentry@anthropics", "user", None).unwrap(),
            "updated"
        );
    }

    #[test]
    fn update_with_an_error_outcome_returns_the_cli_message() {
        let runner = FakeRunner {
            stdout: r#"{"outcome":"error","message":"Plugin not found"}"#.to_string(),
            ..Default::default()
        };
        let err = update_plugin_with(&runner, "sentry@anthropics", "user", None).unwrap_err();
        assert_eq!(err, "Plugin not found");
    }

    #[test]
    fn unrelated_errors_mentioning_confirm_or_tty_keep_their_message() {
        for message in ["could not confirm the marketplace", "tty allocation failed"] {
            let runner = FakeRunner {
                fail: Some(message.to_string()),
                ..Default::default()
            };
            let err = update_plugin_with(&runner, "codex@anthropics", "user", None).unwrap_err();
            assert_eq!(err, message);
        }
    }

    #[test]
    fn managed_update_is_refused_without_running_anything() {
        let runner = FakeRunner::default();
        let err = update_plugin_with(&runner, "codex@anthropics", "managed", None).unwrap_err();
        assert_eq!(err, "This plugin is managed by your organization.");
        assert!(runner.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn project_update_without_a_project_folder_runs_nothing() {
        let runner = FakeRunner::default();
        assert!(update_plugin_with(&runner, "codex@anthropics", "project", None).is_err());
        assert!(runner.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn update_with_an_invalid_plugin_id_runs_nothing() {
        let runner = FakeRunner::default();
        let err = update_plugin_with(&runner, "codex@", "user", None).unwrap_err();
        assert!(err.contains("plugin"));
        assert!(runner.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn update_that_needs_marketplace_command_confirmation_points_to_the_terminal() {
        let runner = FakeRunner {
            fail: Some("This update runs an install command. Pass --yes to accept it.".to_string()),
            ..Default::default()
        };
        let err = update_plugin_with(&runner, "codex@anthropics", "user", None).unwrap_err();
        assert_eq!(
            err,
            "This update runs a command from the plugin's marketplace that needs your OK. Run `claude plugin update codex@anthropics` in a terminal to review it."
        );
    }

    #[test]
    fn update_with_claude_missing_names_the_missing_binary() {
        let runner = FakeRunner {
            fail: Some("No such file or directory (os error 2)".to_string()),
            ..Default::default()
        };
        let err = update_plugin_with(&runner, "codex@anthropics", "user", None).unwrap_err();
        assert!(err.contains("`claude`"));
    }

    #[test]
    fn an_unrelated_update_failure_keeps_its_message() {
        let runner = FakeRunner {
            fail: Some("Plugin not found in marketplace".to_string()),
            ..Default::default()
        };
        let err = update_plugin_with(&runner, "codex@anthropics", "user", None).unwrap_err();
        assert_eq!(err, "Plugin not found in marketplace");
    }
}
