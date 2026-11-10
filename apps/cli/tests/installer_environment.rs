#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/headless.rs"]
mod headless;

use serde_json::json;
use std::path::{Path, PathBuf};

fn executable(name: &str) -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| panic!("{name} is required for the installer environment probe"))
}

fn tree(path: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(tree(&path));
        } else {
            files.push((path.clone(), std::fs::read(path).unwrap()));
        }
    }
    files.sort();
    files
}

fn poisoned_installer(surface: &str, scoped: bool) {
    let home = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let npm_user = outside.path().join("user.npmrc");
    let npm_global = outside.path().join("global.npmrc");
    let git_config = outside.path().join("gitconfig");
    for file in [&npm_user, &npm_global] {
        std::fs::write(file, "registry=https://outside.invalid/\n").unwrap();
    }
    std::fs::write(&git_config, "[fixture]\n scope = outside\n").unwrap();
    let mut env = Vec::new();
    for (key, path) in [
        ("userconfig", npm_user),
        ("globalconfig", npm_global),
        ("cache", outside.path().join("cache")),
        ("prefix", outside.path().join("prefix")),
        ("logs_dir", outside.path().join("logs")),
        ("init_module", outside.path().join("init.js")),
        ("extra_path_setting", outside.path().join("future")),
    ] {
        for prefix in ["npm_config_", "NPM_CONFIG_"] {
            let name = if prefix == "NPM_CONFIG_" {
                format!("{prefix}{}", key.to_uppercase())
            } else {
                format!("{prefix}{key}")
            };
            env.push((name, path.display().to_string()));
        }
    }
    for key in [
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_SYSTEM",
        "GIT_CONFIG",
        "GIT_CONFIG_VALUE_0",
    ] {
        env.push((key.to_string(), git_config.display().to_string()));
    }
    env.extend([
        ("GIT_CONFIG_COUNT".to_string(), "1".to_string()),
        ("GIT_CONFIG_KEY_0".to_string(), "include.path".to_string()),
        (
            "GIT_CONFIG_PARAMETERS".to_string(),
            format!("'include.path'='{}'", git_config.display()),
        ),
        ("GIT_CONFIG_NOSYSTEM".to_string(), "0".to_string()),
    ]);
    let bin = home.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    std::os::unix::fs::symlink("/bin/sh", bin.join("npx")).unwrap();
    let path = std::env::join_paths(
        std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    env.extend([
        ("PATH".to_string(), path.to_str().unwrap().to_string()),
        ("PROBE_HOME".to_string(), home.path().display().to_string()),
        (
            "PROBE_NODE".to_string(),
            executable("node").display().to_string(),
        ),
        (
            "PROBE_NPM".to_string(),
            executable("npm").display().to_string(),
        ),
        (
            "PROBE_GIT".to_string(),
            executable("git").display().to_string(),
        ),
    ]);
    let script = r#"
"$PROBE_NODE" -e 'require("fs").writeFileSync(process.env.PROBE_HOME+"/child-env.json",JSON.stringify(process.env))'
"$PROBE_NPM" --loglevel=silly config get registry > "$PROBE_HOME/npm-registry"
"$PROBE_NPM" --loglevel=silly config get cache > "$PROBE_HOME/npm-cache"
"$PROBE_GIT" config --get fixture.scope > "$PROBE_HOME/git-scope" || :
mkdir -p "$HOME/.agents/skills/gamma"
printf '%s\n' '---' 'name: gamma' 'description: probe' '---' 'Body' > "$HOME/.agents/skills/gamma/SKILL.md"
"#;
    std::fs::write(
        home.path().join("skills"),
        format!(
            "exec /bin/sh -c '{}' \"$0\" \"$@\"\n",
            script.replace('\'', "'\\''")
        ),
    )
    .unwrap();
    std::os::unix::fs::symlink("skills", home.path().join("skills@1.7.0")).unwrap();
    let before = tree(outside.path());
    let args = [
        "add",
        "owner/repo",
        "--method",
        "skills-sh",
        "--name",
        "gamma",
    ];
    let result = if surface == "cli" {
        if scoped {
            headless::cli_with_env(home.path(), &args, &env)
        } else {
            headless::cli_request(home.path(), &args, &env, false)
        }
    } else {
        let req = json!({"skill":"gamma","method":"skills_sh","scope":{"scope":"global"},"source":"owner/repo"});
        let rpc = if scoped {
            headless::mcp_with_env(home.path(), "add", &req, &env)
        } else {
            headless::mcp_request(home.path(), "add", &req, &env, false)
        };
        rpc["result"]["structuredContent"].clone()
    };
    assert_eq!(result["status"], "ok", "{surface}: {result}");
    let child: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&std::fs::read(home.path().join("child-env.json")).unwrap())
            .unwrap();
    let registry = std::fs::read_to_string(home.path().join("npm-registry")).unwrap();
    let git = std::fs::read_to_string(home.path().join("git-scope")).unwrap();
    if scoped {
        for (key, value) in &child {
            let name = key.to_ascii_lowercase();
            if name.starts_with("npm_config_") {
                assert!(
                    Path::new(value.as_str().unwrap()).starts_with(home.path()),
                    "{surface}: npm override {key} is not scoped"
                );
            }
            if name == "git_config" || name.starts_with("git_config_") {
                assert!(
                    key == "GIT_CONFIG_GLOBAL" || key == "GIT_CONFIG_NOSYSTEM",
                    "{surface}: Git override {key} was inherited"
                );
            }
            assert!(
                !value
                    .as_str()
                    .unwrap()
                    .contains(outside.path().to_str().unwrap()),
                "{surface}: child inherited outside path in {key}"
            );
        }
        for name in ["cache", "userconfig", "globalconfig", "prefix"] {
            for key in [
                format!("npm_config_{name}"),
                format!("NPM_CONFIG_{}", name.to_uppercase()),
            ] {
                assert!(
                    Path::new(child[&key].as_str().unwrap()).starts_with(home.path()),
                    "{surface}: {key} is not scoped"
                );
            }
        }
        assert_eq!(child["GIT_CONFIG_NOSYSTEM"], "1");
        assert!(
            !registry.contains("outside.invalid"),
            "{surface}: npm read the outside config"
        );
        assert!(
            git.trim().is_empty(),
            "{surface}: Git read the outside config"
        );
        assert_eq!(
            tree(outside.path()),
            before,
            "{surface}: child wrote outside --home"
        );
        assert!(Path::new(
            std::fs::read_to_string(home.path().join("npm-cache"))
                .unwrap()
                .trim()
        )
        .starts_with(home.path()));
    } else {
        assert!(
            registry.contains("outside.invalid"),
            "{surface}: default npm config was lost"
        );
        assert!(
            git.contains("outside"),
            "{surface}: default Git config was lost"
        );
        assert_ne!(
            tree(outside.path()),
            before,
            "{surface}: npm cache override was not used"
        );
    }
    let scan = if surface == "cli" {
        headless::cli(home.path(), &["scan"])
    } else {
        headless::mcp(home.path(), "scan", &json!({}))["result"]["structuredContent"].clone()
    };
    assert_eq!(scan["data"]["skills"][0]["name"], "gamma");
}

#[test]
fn cli_scoped_install_rejects_poisoned_npm_and_git_environment_or_names_the_escape() {
    poisoned_installer("cli", true);
}
#[test]
fn mcp_scoped_install_rejects_poisoned_npm_and_git_environment_or_names_the_escape() {
    poisoned_installer("mcp", true);
}
#[test]
fn cli_default_home_preserves_npm_and_git_overrides_or_names_the_lost_config() {
    poisoned_installer("cli", false);
}
#[test]
fn mcp_default_home_preserves_npm_and_git_overrides_or_names_the_lost_config() {
    poisoned_installer("mcp", false);
}
