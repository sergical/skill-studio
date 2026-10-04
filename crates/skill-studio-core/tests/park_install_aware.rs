// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate, so
// the same allow needs to be declared here too.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr
)]

//! Real-disk tests for install-aware Park (#389): a parked skill must stay
//! parked when its installer runs again. The dotagents stub below stands in
//! for the real `dotagents`: `remove` edits `agents.toml` the way the real
//! command does, and `install` recreates every listed, non-excluded skill.
//! Each test names the flow, what must hold, and what a failure means.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use skill_studio_core::dto::{
    DeploymentDto, InstallMethod, ParkRequest, RestoreRequest, ScanRequest, UnparkRequest,
    UpdateRequest,
};
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::identity::{RootKind, RootScope, SkillName};
use skill_studio_core::ops;
use skill_studio_core::ports::{
    CancelToken, NeverCancel, Ports, ProcessOutput, ProcessSpawner, ProcessSpec, Runtime,
};
use skill_studio_core::scope::RuntimeScope;
use skill_studio_core::testing::golden::{ctx, unique_temp_dir};
use skill_studio_core::testing::{FakeClock, FakeIds, FakeToolLookup, RecordingSink};
use skill_studio_core::CoreError;

use skill_studio_host::{FileLease, RealFs, SqliteHistoryOpener};

const DOTAGENTS: &str = "/fake/bin/dotagents";

/// A fake `dotagents` over the temp home. `wildcard_pool` is what a
/// `name = "*"` entry's source holds.
struct FakeDotagents {
    home: PathBuf,
    wildcard_pool: Vec<String>,
    calls: Mutex<Vec<Vec<String>>>,
}

impl FakeDotagents {
    fn new(home: &Path, wildcard_pool: &[&str]) -> Arc<Self> {
        Arc::new(FakeDotagents {
            home: home.to_path_buf(),
            wildcard_pool: wildcard_pool.iter().map(|s| (*s).to_string()).collect(),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }

    fn toml_path(&self) -> PathBuf {
        self.home.join(".agents/agents.toml")
    }

    fn lock_path(&self) -> PathBuf {
        self.home.join(".agents/agents.lock")
    }

    fn skill_dir(&self, name: &str) -> PathBuf {
        self.home.join(".agents/skills").join(name)
    }

    fn write_lock_row(&self, name: &str) {
        let lock = std::fs::read_to_string(self.lock_path()).unwrap_or_default();
        if !lock.contains(&format!("[skills.{name}]")) {
            let lock = format!("{lock}[skills.{name}]\nsource = \"owner/{name}\"\n");
            std::fs::write(self.lock_path(), lock).unwrap();
        }
    }

    fn remove(&self, name: &str) {
        let text = std::fs::read_to_string(self.toml_path()).unwrap();
        let mut doc = text.parse::<toml_edit::DocumentMut>().unwrap();
        let rows = doc
            .get_mut("skills")
            .and_then(toml_edit::Item::as_array_of_tables_mut)
            .unwrap();
        let explicit = rows
            .iter()
            .position(|row| row.get("name").and_then(toml_edit::Item::as_str) == Some(name));
        if let Some(index) = explicit {
            rows.remove(index);
        } else {
            let wildcard = rows
                .iter_mut()
                .find(|row| row.get("name").and_then(toml_edit::Item::as_str) == Some("*"))
                .unwrap();
            if wildcard.get("exclude").is_none() {
                wildcard["exclude"] = toml_edit::value(toml_edit::Array::new());
            }
            wildcard["exclude"]
                .as_array_mut()
                .unwrap()
                .push(name.to_string());
        }
        std::fs::write(self.toml_path(), doc.to_string()).unwrap();
        std::fs::remove_dir_all(self.skill_dir(name)).ok();
        let lock = std::fs::read_to_string(self.lock_path()).unwrap_or_default();
        let mut lock_doc = lock.parse::<toml_edit::DocumentMut>().unwrap();
        if let Some(skills) = lock_doc
            .get_mut("skills")
            .and_then(toml_edit::Item::as_table_mut)
        {
            skills.remove(name);
        }
        std::fs::write(self.lock_path(), lock_doc.to_string()).unwrap();
    }

    fn install(&self) {
        let text = std::fs::read_to_string(self.toml_path()).unwrap();
        let doc = text.parse::<toml_edit::DocumentMut>().unwrap();
        let mut wanted: Vec<String> = Vec::new();
        for row in doc
            .get("skills")
            .and_then(toml_edit::Item::as_array_of_tables)
            .unwrap()
        {
            match row.get("name").and_then(toml_edit::Item::as_str).unwrap() {
                "*" => {
                    let excluded: Vec<&str> = row
                        .get("exclude")
                        .and_then(toml_edit::Item::as_array)
                        .map(|list| list.iter().filter_map(|item| item.as_str()).collect())
                        .unwrap_or_default();
                    wanted.extend(
                        self.wildcard_pool
                            .iter()
                            .filter(|name| !excluded.contains(&name.as_str()))
                            .cloned(),
                    );
                }
                name => wanted.push(name.to_string()),
            }
        }
        for name in wanted {
            let dir = self.skill_dir(&name);
            if !dir.exists() {
                write_skill(&dir, &name);
            }
            self.write_lock_row(&name);
        }
    }
}

impl ProcessSpawner for FakeDotagents {
    fn run(
        &self,
        spec: &ProcessSpec,
        _cancel: &dyn CancelToken,
    ) -> Result<ProcessOutput, CoreError> {
        assert_eq!(spec.program, DOTAGENTS);
        self.calls.lock().unwrap().push(spec.args.clone());
        match spec.args.iter().position(|arg| arg == "remove") {
            Some(index) => self.remove(&spec.args[index + 1]),
            None => self.install(),
        }
        Ok(ProcessOutput {
            status: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
        })
    }
}

/// Runs `dotagents install` through the stub, as a person would in a terminal.
fn dotagents_install(stub: &FakeDotagents) {
    let spec = ProcessSpec {
        program: DOTAGENTS.to_string(),
        args: vec!["install".to_string()],
        cwd: None,
        env: Vec::new(),
        timeout_ms: 1_000,
    };
    stub.run(&spec, &NeverCancel).unwrap();
}

fn write_skill(dir: &Path, name: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: a parkable skill\n---\nBody.\n"),
    )
    .unwrap();
}

fn runtime(home: &Path, spawner: Arc<dyn ProcessSpawner>, dotagents_on_path: bool) -> Runtime {
    let mut tools = FakeToolLookup::default();
    if dotagents_on_path {
        tools
            .binaries
            .insert("dotagents".to_string(), PathBuf::from(DOTAGENTS));
    }
    let ports = Ports {
        fs: Arc::new(RealFs::new()),
        clock: Arc::new(FakeClock::at(0)),
        ids: Arc::new(FakeIds::default()),
        leases: Arc::new(FileLease::new(home.join(".leases"))),
        history: Arc::new(SqliteHistoryOpener::new(
            home.join(".history").join("events.sqlite3"),
        )),
        sink: Arc::new(RecordingSink::default()),
        spawner: Some(spawner),
        discovery: None,
        tools: Some(Arc::new(tools)),
        catalog: Arc::new(HarnessCatalog::builtin()),
        telemetry: Arc::new(skill_studio_core::ports::NoopTelemetry),
    };
    Runtime::new(&RuntimeScope::fixture(home), ports).unwrap()
}

fn copies(rt: &Runtime, name: &str) -> Vec<DeploymentDto> {
    ops::scan(rt, &ctx(), &ScanRequest::default())
        .unwrap()
        .skills
        .into_iter()
        .filter(|skill| skill.name.0 == name)
        .flat_map(|skill| skill.deployments)
        .collect()
}

fn live_copy(rt: &Runtime, name: &str) -> DeploymentDto {
    copies(rt, name)
        .into_iter()
        .find(|d| d.root.kind == RootKind::Universal)
        .unwrap_or_else(|| panic!("no live Universal copy of {name}"))
}

fn parked_copy(rt: &Runtime, name: &str) -> DeploymentDto {
    copies(rt, name)
        .into_iter()
        .find(|d| d.root.kind == RootKind::Parked)
        .unwrap_or_else(|| panic!("no parked copy of {name}"))
}

const WILDCARD_TOML: &str =
    "# my dotagents setup\nversion = 1\n\n[[skills]]\nname = \"*\"\nsource = \"owner/pack\"\n";
const EXPLICIT_TOML: &str = "# my dotagents setup\nversion = 1\n\n[[skills]]\nname = \"foo\"\nsource = \"owner/foo\"\n\n[[skills]]\nname = \"bar\"\nsource = \"owner/bar\"\n";

/// A temp home where dotagents has installed `foo` and `bar` from `toml`.
fn dotagents_home(label: &str, toml: &str) -> (PathBuf, Arc<FakeDotagents>) {
    let home = unique_temp_dir(label);
    std::fs::create_dir_all(home.join(".agents")).unwrap();
    std::fs::write(home.join(".agents/agents.toml"), toml).unwrap();
    let stub = FakeDotagents::new(&home, &["foo", "bar"]);
    dotagents_install(&stub);
    (home, stub)
}

fn park_foo(rt: &Runtime) -> skill_studio_core::dto::ParkOutcome {
    ops::park(
        rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live_copy(rt, "foo").id,
        },
    )
    .unwrap()
}

fn unpark_foo(rt: &Runtime) {
    ops::unpark(
        rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked_copy(rt, "foo").id,
        },
    )
    .unwrap();
}

/// Flow: park a skill a `name = "*"` entry supplies, run `dotagents install`,
/// then turn the skill on and run it again. Expectation: install leaves the
/// parked skill parked, turn-on takes it out of `exclude`, and the next
/// install keeps it. Failure: dotagents brings a parked skill back, or
/// turn-on leaves the skill excluded for good.
#[test]
fn wildcard_skill_stays_parked_through_dotagents_install_and_returns_on_turn_on() {
    let (home, stub) = dotagents_home("park_dotagents_wildcard", WILDCARD_TOML);
    let rt = runtime(&home, stub.clone(), true);
    let live = home.join(".agents/skills/foo");

    park_foo(&rt);
    assert_eq!(stub.calls().last().unwrap(), &["remove", "foo"]);
    assert!(std::fs::read_to_string(stub.toml_path())
        .unwrap()
        .contains("exclude"));
    dotagents_install(&stub);
    assert!(
        !live.exists(),
        "dotagents install brought the parked skill back"
    );
    assert!(home.join(".agents/skills/bar").exists());
    assert!(home
        .join(".agents/skills-parked/universal/foo/SKILL.md")
        .exists());

    unpark_foo(&rt);
    let toml = std::fs::read_to_string(stub.toml_path()).unwrap();
    assert_eq!(
        toml, WILDCARD_TOML,
        "turn-on must restore agents.toml as it was"
    );
    std::fs::remove_dir_all(&live).unwrap();
    dotagents_install(&stub);
    assert!(
        live.join("SKILL.md").exists(),
        "after turn-on dotagents install must manage the skill again"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: same as the wildcard test, for a skill `agents.toml` lists by name.
/// Expectation: park drops the entry and keeps the rest of the file (comment
/// and the `bar` entry); turn-on lists `foo` again. Failure: install revives
/// the skill, a neighbouring entry or comment is lost, or the entry does not
/// come back.
#[test]
fn explicit_skill_stays_parked_through_dotagents_install_and_returns_on_turn_on() {
    let (home, stub) = dotagents_home("park_dotagents_explicit", EXPLICIT_TOML);
    let rt = runtime(&home, stub.clone(), true);
    let live = home.join(".agents/skills/foo");

    park_foo(&rt);
    let toml = std::fs::read_to_string(stub.toml_path()).unwrap();
    assert!(!toml.contains("name = \"foo\""));
    assert!(toml.contains("# my dotagents setup") && toml.contains("name = \"bar\""));
    dotagents_install(&stub);
    assert!(
        !live.exists(),
        "dotagents install brought the parked skill back"
    );

    unpark_foo(&rt);
    let toml = std::fs::read_to_string(stub.toml_path()).unwrap();
    assert!(toml.contains("name = \"foo\"") && toml.contains("source = \"owner/foo\""));
    assert!(toml.contains("# my dotagents setup") && toml.contains("name = \"bar\""));
    std::fs::remove_dir_all(&live).unwrap();
    dotagents_install(&stub);
    assert!(
        live.join("SKILL.md").exists(),
        "after turn-on dotagents install must manage the skill again"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a dotagents skill, then undo the park event. Expectation:
/// `agents.toml` is byte-for-byte what it was before the park. Failure: the
/// park row has no inverse, or undo leaves the entry removed or excluded.
#[test]
fn undo_of_a_dotagents_park_restores_agents_toml() {
    for toml in [WILDCARD_TOML, EXPLICIT_TOML] {
        let (home, stub) = dotagents_home("park_dotagents_undo", toml);
        let rt = runtime(&home, stub.clone(), true);

        let parked = park_foo(&rt);
        assert_ne!(std::fs::read_to_string(stub.toml_path()).unwrap(), toml);
        ops::restore_event(
            &rt,
            &ctx(),
            &RestoreRequest {
                event_id: parked.event_id,
                force: false,
            },
        )
        .unwrap();

        assert_eq!(std::fs::read_to_string(stub.toml_path()).unwrap(), toml);
        std::fs::remove_dir_all(&home).ok();
    }
}

/// Flow: park a dotagents skill on a machine with no `dotagents` on `PATH`.
/// Expectation: park is refused with a reason that names dotagents, the copy
/// stays where it is, `agents.toml` is untouched, and nothing ran. Failure:
/// the copy is parked anyway and the next `dotagents install` brings it back.
#[test]
fn park_of_a_dotagents_skill_is_refused_with_a_plain_reason_when_dotagents_is_missing() {
    let (home, stub) = dotagents_home("park_dotagents_missing", WILDCARD_TOML);
    let rt = runtime(&home, stub.clone(), false);
    let calls_before = stub.calls().len();

    let err = ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live_copy(&rt, "foo").id,
        },
    )
    .unwrap_err();

    assert!(
        err.message.contains("but is not installed here"),
        "{}",
        err.message
    );
    assert!(home.join(".agents/skills/foo/SKILL.md").exists());
    assert_eq!(
        std::fs::read_to_string(stub.toml_path()).unwrap(),
        WILDCARD_TOML
    );
    assert_eq!(stub.calls().len(), calls_before);
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a skill the skills CLI installed, then ask the app to update
/// it. Expectation: the update is refused with a plain reason, the CLI never
/// runs, the folder is not recreated, and the parked copy and
/// `.skill-lock.json` stay as they were. Failure: Update re-fetches a parked
/// skill and undoes the park.
#[test]
fn app_update_skips_a_parked_skills_cli_skill() {
    let home = unique_temp_dir("park_skills_cli_update");
    let live = home.join(".agents/skills/foo");
    write_skill(&live, "foo");
    let lock = "{\"version\":3,\"skills\":{\"foo\":{\"source\":\"owner/foo\",\"sourceType\":\"github\",\"skillFolderHash\":\"abc\"}}}";
    std::fs::write(home.join(".agents/.skill-lock.json"), lock).unwrap();
    let stub = FakeDotagents::new(&home, &[]);
    let rt = runtime(&home, stub.clone(), true);

    park_foo(&rt);
    let err = ops::update(
        &rt,
        &ctx(),
        &UpdateRequest {
            skill: SkillName("foo".to_string()),
            method: InstallMethod::SkillsSh,
            scope: RootScope::Global,
            files: Vec::new(),
            source: None,
            ref_pin: None,
        },
    )
    .unwrap_err();

    assert!(err.message.contains("is parked"), "{}", err.message);
    assert!(stub.calls().is_empty(), "the CLI ran for a parked skill");
    assert!(!live.exists());
    assert!(home
        .join(".agents/skills-parked/universal/foo/SKILL.md")
        .exists());
    assert_eq!(
        std::fs::read_to_string(home.join(".agents/.skill-lock.json")).unwrap(),
        lock
    );
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a copy placed by hand. Expectation: no installer is called.
/// Failure: park shells out for a skill no installer manages.
#[test]
fn park_of_a_hand_placed_skill_runs_no_installer() {
    let home = unique_temp_dir("park_by_hand");
    write_skill(&home.join(".agents/skills/foo"), "foo");
    let stub = FakeDotagents::new(&home, &[]);
    let rt = runtime(&home, stub.clone(), true);

    park_foo(&rt);

    assert!(stub.calls().is_empty());
    std::fs::remove_dir_all(&home).ok();
}
