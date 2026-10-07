// ============================================================================
// Skills Module - skill_plugin_lifecycle
// Claude Code plugin actions: disable, enable, update, and uninstall a plugin
// through the scriptable `claude plugin` CLI. Every skill a plugin ships
// moves together, since Claude Code tracks the switch per plugin, not per
// skill. Codex has no plugin CLI - plugins there are managed with
// `/plugins` inside a Codex session, so this module only ever runs `claude`.
// ============================================================================

use std::path::Path;

use serde::Serialize;

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

/// `claude plugin marketplace update <marketplace>`: re-reads the marketplace
/// from its source, since `claude plugin update` compares against the local
/// marketplace checkout.
pub fn plugin_marketplace_update_args(marketplace: &str) -> Vec<String> {
    vec![
        "plugin".to_string(),
        "marketplace".to_string(),
        "update".to_string(),
        marketplace.to_string(),
    ]
}

/// What `claude plugin update --json` reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginUpdateResult {
    /// The CLI's `updateOutcome`: `updated`, `up_to_date`, `skipped`, ...
    pub outcome: String,
    /// The CLI's own `message` (or `reason`) for the outcome, when it gave one.
    pub message: Option<String>,
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
            && !part.starts_with('-')
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

/// The `message` of a `{"outcome":"error","message":...}` line, when `text`
/// holds one.
fn json_error_message(text: &str) -> Option<String> {
    text.lines().rev().find_map(|line| {
        let value = serde_json::from_str::<serde_json::Value>(line.trim()).ok()?;
        if value.get("outcome")?.as_str()? != "error" {
            return None;
        }
        value.get("message")?.as_str().map(str::to_string)
    })
}

/// Rewrites the CLI's refusal to run a marketplace-declared command without
/// confirmation (`-y`, or a TTY) into the next step for the person.
fn update_error(plugin_id: &str, scope: &str, cwd: Option<&Path>, message: String) -> String {
    // A failed run can still print the `--json` error object on stdout.
    let message = json_error_message(&message).unwrap_or(message);
    let lower = message.to_lowercase();
    // The wording of `claude plugin update --help` for `-y` and
    // `--accept-command`; bare "confirm" or "tty" also match unrelated errors.
    let needs_confirmation =
        lower.contains("--yes") || lower.contains("pass -y") || lower.contains("--accept-command");
    if needs_confirmation {
        // Without `-s` the CLI picks the scope itself, so a project install run
        // from elsewhere could update the user copy instead.
        let cd = cwd
            .map(|dir| format!("cd {} && ", shell_quote(&dir.to_string_lossy())))
            .unwrap_or_default();
        format!(
            "This update runs a command from the plugin's marketplace that needs your OK. Run `{cd}claude plugin update {plugin_id} -s {scope}` in a terminal to review it."
        )
    } else {
        friendly_error(message)
    }
}

/// `text` as one POSIX shell word.
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
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
/// `updateOutcome` (`updated`, `up_to_date`, ...) and the CLI's message. An
/// `outcome` of `error` becomes the CLI's own message. Output that is not JSON
/// counts as `updated`, since a zero exit code already said the command worked.
fn parse_update_outcome(stdout: &[u8]) -> Result<PluginUpdateResult, String> {
    let text = String::from_utf8_lossy(stdout);
    let parsed = text
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok());
    let Some(value) = parsed else {
        return Ok(PluginUpdateResult {
            outcome: "updated".to_string(),
            message: None,
        });
    };
    let text_field = |name: &str| {
        value
            .get(name)
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    if value.get("outcome").and_then(|v| v.as_str()) == Some("error") {
        return Err(
            text_field("message").unwrap_or_else(|| "The plugin update failed.".to_string())
        );
    }
    Ok(PluginUpdateResult {
        outcome: text_field("updateOutcome").unwrap_or_else(|| "updated".to_string()),
        message: text_field("message").or_else(|| text_field("reason")),
    })
}

/// Runs `claude plugin update` for one install of a plugin and returns the
/// CLI's `updateOutcome` and message. The plugin's marketplace is refreshed
/// first, because the update reads the local marketplace checkout; a failed
/// refresh does not stop the update, but an `up_to_date` result after one
/// becomes `marketplace_stale`, since the checkout may be old. `scope` is the install's own scope from
/// `installed_plugins.json`; a project or local install runs in its project
/// folder, where Claude Code resolves it.
pub fn update_plugin_with(
    runner: &dyn CommandRunner,
    plugin_id: &str,
    scope: &str,
    project_path: Option<&Path>,
) -> Result<PluginUpdateResult, String> {
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
    let refresh_error = plugin_id.split_once('@').and_then(|(_, marketplace)| {
        runner
            .run(
                CLAUDE_CLI,
                &plugin_marketplace_update_args(marketplace),
                None::<&Path>,
            )
            .err()
            .map(|message| (marketplace, friendly_error(message)))
    });
    let stdout = runner
        .run_output(CLAUDE_CLI, &plugin_update_args(plugin_id, scope), cwd)
        .map_err(|message| update_error(plugin_id, scope, cwd, message))?;
    let mut result = parse_update_outcome(&stdout)?;
    if let (Some((marketplace, error)), "up_to_date") = (refresh_error, result.outcome.as_str()) {
        result.outcome = "marketplace_stale".to_string();
        result.message = Some(format!(
            "Could not refresh the {marketplace} marketplace ({error}), so this plugin may have a newer version."
        ));
    }
    Ok(result)
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
        assert_eq!(calls[1].0, "claude");
        assert_eq!(
            calls[1].1,
            vec![
                "plugin",
                "update",
                "sentry@claude-plugins-official",
                "-s",
                "user",
                "--json"
            ]
        );
        assert!(!calls[1].1.iter().any(|a| a == "-y" || a == "--yes"));
        assert_eq!(runner.cwds.lock().unwrap()[1], None);
    }

    /// Flow: update a plugin whose marketplace checkout may be stale.
    /// Expectation: `claude plugin marketplace update <marketplace>` runs
    /// first, and a failing refresh still lets the plugin update run.
    /// A failure means updates compare against an unrefreshed checkout, or a
    /// refresh error blocks the update.
    #[test]
    fn update_refreshes_the_marketplace_first_and_survives_a_failed_refresh() {
        let runner = FakeRunner::default();
        update_plugin_with(&runner, "sentry@claude-plugins-official", "user", None).unwrap();
        let calls = runner.calls.lock().unwrap();
        assert_eq!(
            calls[0].1,
            vec!["plugin", "marketplace", "update", "claude-plugins-official"]
        );
        assert_eq!(calls[1].1[1], "update");
        drop(calls);

        struct RefreshFails(FakeRunner);
        impl CommandRunner for RefreshFails {
            fn run(
                &self,
                program: &str,
                args: &[String],
                cwd: Option<&Path>,
            ) -> Result<(), String> {
                self.0.run(program, args, cwd)?;
                if args.get(1).map(String::as_str) == Some("marketplace") {
                    return Err("network down".to_string());
                }
                Ok(())
            }
            fn run_output(
                &self,
                program: &str,
                args: &[String],
                cwd: Option<&Path>,
            ) -> Result<Vec<u8>, String> {
                self.run(program, args, cwd)?;
                Ok(Vec::new())
            }
        }
        let runner = RefreshFails(FakeRunner::default());
        let result = update_plugin_with(&runner, "sentry@anthropics", "user", None).unwrap();
        assert_eq!(result.outcome, "updated");
        assert_eq!(runner.0.calls.lock().unwrap().len(), 2);
    }

    /// Flow: the marketplace refresh fails and the plugin update then says
    /// `up_to_date`, or says `updated`.
    /// Expectation: `up_to_date` becomes `marketplace_stale` with a message
    /// naming the marketplace and the refresh error; `updated` stays.
    /// A failure means a stale checkout is reported as "already current".
    #[test]
    fn a_failed_refresh_turns_up_to_date_into_marketplace_stale() {
        struct RefreshFails(FakeRunner);
        impl CommandRunner for RefreshFails {
            fn run(
                &self,
                program: &str,
                args: &[String],
                cwd: Option<&Path>,
            ) -> Result<(), String> {
                self.0.run(program, args, cwd)?;
                if args.get(1).map(String::as_str) == Some("marketplace") {
                    return Err("network down".to_string());
                }
                Ok(())
            }
            fn run_output(
                &self,
                program: &str,
                args: &[String],
                cwd: Option<&Path>,
            ) -> Result<Vec<u8>, String> {
                self.run(program, args, cwd)?;
                Ok(self.0.stdout.clone().into_bytes())
            }
        }
        let current = RefreshFails(FakeRunner {
            stdout: r#"{"updateOutcome":"up_to_date"}"#.to_string(),
            ..Default::default()
        });
        let result = update_plugin_with(&current, "sentry@anthropics", "user", None).unwrap();
        assert_eq!(result.outcome, "marketplace_stale");
        let message = result.message.unwrap();
        assert!(message.contains("anthropics") && message.contains("network down"));

        let updated = RefreshFails(FakeRunner {
            stdout: r#"{"updateOutcome":"updated"}"#.to_string(),
            ..Default::default()
        });
        let result = update_plugin_with(&updated, "sentry@anthropics", "user", None).unwrap();
        assert_eq!(result.outcome, "updated");
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
            runner.calls.lock().unwrap()[1].1,
            vec![
                "plugin",
                "update",
                "plugin-dev@claude-plugins-official",
                "-s",
                "project",
                "--json"
            ]
        );
        assert_eq!(runner.cwds.lock().unwrap()[1].as_deref(), Some(project));
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
            stdout: r#"{"outcome":"ok","updateOutcome":"up_to_date","oldVersion":"1.4.0","newVersion":"1.4.0"}"#
                .to_string(),
            ..Default::default()
        };
        let outcome = update_plugin_with(&runner, "sentry@anthropics", "user", None).unwrap();
        assert_eq!(outcome.outcome, "up_to_date");
        assert_eq!(outcome.message, None);

        let runner = FakeRunner {
            stdout: r#"{"outcome":"ok","updateOutcome":"updated"}"#.to_string(),
            ..Default::default()
        };
        assert_eq!(
            update_plugin_with(&runner, "sentry@anthropics", "user", None)
                .unwrap()
                .outcome,
            "updated"
        );
    }

    /// Flow: the CLI skips an update and says why.
    /// Expectation: the outcome stays `skipped` and the CLI's message is kept,
    /// so the app can show it instead of claiming success.
    #[test]
    fn a_skipped_outcome_keeps_the_cli_message() {
        let runner = FakeRunner {
            stdout: r#"{"outcome":"ok","updateOutcome":"skipped","message":"Pinned to 1.0.0"}"#
                .to_string(),
            ..Default::default()
        };
        let result = update_plugin_with(&runner, "sentry@anthropics", "user", None).unwrap();
        assert_eq!(
            result,
            PluginUpdateResult {
                outcome: "skipped".to_string(),
                message: Some("Pinned to 1.0.0".to_string()),
            }
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

    /// Flow: a plugin id whose marketplace half looks like a flag.
    /// Expect: rejected before `claude plugin marketplace update` sees it.
    /// Fails if: `-h` reaches the CLI as an option instead of a name.
    #[test]
    fn update_with_a_flag_like_marketplace_runs_nothing() {
        let runner = FakeRunner::default();
        assert!(update_plugin_with(&runner, "codex@-h", "user", None).is_err());
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
            "This update runs a command from the plugin's marketplace that needs your OK. Run `claude plugin update codex@anthropics -s user` in a terminal to review it."
        );
    }

    /// Flow: a project install's update needs the marketplace command confirmed.
    /// Expect: the terminal hint changes into the (shell-quoted) project folder and names the scope.
    /// Fails if: the hint drops either, so running it updates the user copy instead.
    #[test]
    fn a_project_confirmation_hint_keeps_the_scope_and_folder() {
        let parent = tempfile::tempdir().unwrap();
        let project = parent.path().join("it's app");
        std::fs::create_dir(&project).unwrap();
        let runner = FakeRunner {
            fail: Some("Pass --yes to accept the install command.".to_string()),
            ..Default::default()
        };
        let err =
            update_plugin_with(&runner, "codex@anthropics", "project", Some(&project)).unwrap_err();
        let quoted = format!("'{}/it'\\''s app'", parent.path().display());
        assert!(
            err.contains(&format!(
                "Run `cd {quoted} && claude plugin update codex@anthropics -s project`"
            )),
            "{err}"
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

    /// Flow: `claude plugin update --json` exits non-zero and prints the error
    /// object on stdout. Expectation: the person sees its message, not JSON.
    /// A failure means the failed-exit path shows raw JSON again.
    #[test]
    fn a_failed_exit_with_json_on_stdout_shows_its_message() {
        let runner = FakeRunner {
            fail: Some(r#"{"outcome":"error","message":"Plugin not found"}"#.to_string()),
            ..Default::default()
        };
        let err = update_plugin_with(&runner, "codex@anthropics", "user", None).unwrap_err();
        assert_eq!(err, "Plugin not found");
    }
}
