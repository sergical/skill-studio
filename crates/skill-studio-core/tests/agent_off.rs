// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Real-disk integration tests for `ops_agent_off::turn_off_for_agent` and
//! its undo through `ops::restore_event`. Real adapters, because the op
//! renames folders and removes links that the in-memory `FixtureFs` cannot
//! stand in for.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use skill_studio_core::dto::{
    AgentOffRequest, DeploymentDto, ListEventsRequest, RestoreCapability, RestoreRequest,
    ScanRequest, UnparkRequest,
};
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::identity::{AgentId, DeploymentId, RootKind};
use skill_studio_core::ops;
use skill_studio_core::ops_agent_off::{turn_off_check, turn_off_for_agent};
use skill_studio_core::ports::{Ports, Runtime};
use skill_studio_core::scope::{ProjectSelection, RuntimeScope};
use skill_studio_core::testing::golden::{ctx, unique_temp_dir};
use skill_studio_core::testing::{FakeClock, FakeIds, RecordingSink};

use skill_studio_host::{FileLease, RealFs, SqliteHistoryOpener};

const SKILL_MD: &[u8] = b"---\nname: gamma\ndescription: a skill to turn off\n---\nBody.\n";

fn runtime(home: &Path, projects: &[PathBuf]) -> Runtime {
    let mut scope = RuntimeScope::fixture(home);
    scope.projects = ProjectSelection::Explicit {
        paths: projects.to_vec(),
    };
    let ports = Ports {
        fs: Arc::new(RealFs::new()),
        clock: Arc::new(FakeClock::at(0)),
        ids: Arc::new(FakeIds::default()),
        leases: Arc::new(FileLease::new(home.join(".leases"))),
        history: Arc::new(SqliteHistoryOpener::new(
            home.join(".history").join("events.sqlite3"),
        )),
        sink: Arc::new(RecordingSink::default()),
        spawner: None,
        discovery: None,
        tools: None,
        catalog: Arc::new(HarnessCatalog::builtin()),
        telemetry: Arc::new(skill_studio_core::ports::NoopTelemetry),
    };
    Runtime::new(&scope, ports).unwrap()
}

/// A shared skill `gamma` (SKILL.md plus a nested file) under `shared_root`,
/// linked per skill from `linked_roots`.
fn shared_skill(shared_root: &Path, linked_roots: &[PathBuf]) {
    let dir = shared_root.join("gamma");
    std::fs::create_dir_all(dir.join("refs")).unwrap();
    std::fs::write(dir.join("SKILL.md"), SKILL_MD).unwrap();
    std::fs::write(dir.join("refs/notes.md"), b"notes\n").unwrap();
    for root in linked_roots {
        std::fs::create_dir_all(root).unwrap();
        std::os::unix::fs::symlink(&dir, root.join("gamma")).unwrap();
    }
}

fn global_home(home: &Path) {
    shared_skill(
        &home.join(".agents/skills"),
        &[home.join(".claude/skills"), home.join(".pi/agent/skills")],
    );
}

fn shared_deployment(rt: &Runtime) -> DeploymentDto {
    let inventory = ops::scan(rt, &ctx(), &ScanRequest::default()).unwrap();
    inventory
        .skills
        .into_iter()
        .find(|s| s.name.0 == "gamma")
        .unwrap()
        .deployments
        .into_iter()
        .find(|d| d.root.kind == RootKind::Universal)
        .unwrap()
}

fn request(deployment_id: DeploymentId, agent: &str) -> AgentOffRequest {
    AgentOffRequest {
        deployment_id,
        agent: AgentId::parse(agent).unwrap(),
    }
}

fn kinds(rt: &Runtime) -> Vec<String> {
    ops::list_events(rt, &ctx(), &ListEventsRequest::default())
        .unwrap()
        .iter()
        .map(|e| e.kind.clone())
        .collect()
}

fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_dir())
}

fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// Every file under `dir`, relative path to bytes.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    std::fs::read(&path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

/// Every agent but Codex that reads `global_home`'s shared folder: Claude
/// Code and pi by link, the rest by reading the folder.
const READERS_BESIDES_CODEX: [&str; 5] = [
    ".claude/skills/gamma",
    ".pi/agent/skills/gamma",
    ".config/opencode/skills/gamma",
    ".cursor/skills/gamma",
    ".grok/skills/gamma",
];

/// Flow: `gamma` sits in the shared folder, linked from Claude Code and pi.
/// Turn it off for Codex. Expect a real copy for every other reader
/// (Claude Code, pi, `OpenCode`, Cursor, Grok Build), no copy at Codex's
/// folder, the Codex copy parked with its `.origin` note, no shared folder,
/// and the split row (marked as Codex's turn-off) plus the park row Unpark
/// needs. Catches a flow that leaves an agent without the skill or parks
/// the wrong copy.
#[test]
fn turning_off_codex_keeps_every_other_agent_live_and_parks_the_codex_copy_in_one_event() {
    let home = unique_temp_dir("agent_off_codex");
    global_home(&home);
    let rt = runtime(&home, &[]);
    let id = shared_deployment(&rt).id;

    let outcome = turn_off_for_agent(&rt, &ctx(), &request(id, "codex")).unwrap();

    let parked = home.join(".agents/skills-parked/codex/gamma");
    assert_eq!(outcome.parked_path, parked);
    assert_eq!(std::fs::read(parked.join("SKILL.md")).unwrap(), SKILL_MD);
    assert_eq!(
        std::fs::read_to_string(home.join(".agents/skills-parked/codex/.origin/gamma")).unwrap(),
        ".codex/skills"
    );
    assert!(std::fs::symlink_metadata(home.join(".codex/skills/gamma")).is_err());
    for live in READERS_BESIDES_CODEX {
        let live = home.join(live);
        assert!(is_real_dir(&live), "{} must be a real copy", live.display());
        assert_eq!(std::fs::read(live.join("SKILL.md")).unwrap(), SKILL_MD);
    }
    assert!(std::fs::symlink_metadata(home.join(".agents/skills/gamma")).is_err());
    let mut kinds = kinds(&rt);
    kinds.sort();
    assert_eq!(kinds, vec!["park".to_string(), "split".to_string()]);
    let split_row = ops::list_events(&rt, &ctx(), &ListEventsRequest::default())
        .unwrap()
        .into_iter()
        .find(|e| e.kind == "split")
        .unwrap();
    assert_eq!(split_row.harness, Some(AgentId::parse("codex").unwrap()));

    std::fs::remove_dir_all(&home).ok();
}

/// Flow: turn `gamma` off for Codex, then undo the one event. Expect the
/// shared folder back byte for byte, the Claude Code and pi links back, and
/// no per-agent copy, parked copy or `.origin` note. Catches an undo that
/// leaves the parked copy behind (the skill would be both on and parked) or
/// restores different bytes.
#[test]
fn undo_after_turning_off_codex_restores_the_shared_folder_and_removes_every_new_copy() {
    let home = unique_temp_dir("agent_off_undo");
    global_home(&home);
    let before = tree(&home.join(".agents/skills/gamma"));
    let rt = runtime(&home, &[]);
    let id = shared_deployment(&rt).id;
    let outcome = turn_off_for_agent(&rt, &ctx(), &request(id, "codex")).unwrap();

    ops::restore_event(
        &rt,
        &ctx(),
        &RestoreRequest {
            event_id: outcome.event_id,
            force: false,
        },
    )
    .unwrap();

    assert_eq!(tree(&home.join(".agents/skills/gamma")), before);
    for link in [".claude/skills/gamma", ".pi/agent/skills/gamma"] {
        assert!(is_link(&home.join(link)), "{link} must be a link again");
    }
    for copy in READERS_BESIDES_CODEX.iter().skip(2) {
        assert!(std::fs::symlink_metadata(home.join(copy)).is_err());
    }
    assert!(std::fs::symlink_metadata(home.join(".codex/skills/gamma")).is_err());
    assert!(std::fs::symlink_metadata(home.join(".agents/skills-parked/codex/gamma")).is_err());
    assert!(
        std::fs::symlink_metadata(home.join(".agents/skills-parked/codex/.origin/gamma")).is_err()
    );

    std::fs::remove_dir_all(&home).ok();
}

/// Flow: turn `gamma` off for Codex, edit the Claude Code copy, then undo.
/// Expect a refusal without `force` and the edit untouched. Catches an undo
/// that deletes work the user did after the change.
#[test]
fn undo_refuses_when_a_copy_changed_since_and_force_restores() {
    let home = unique_temp_dir("agent_off_undo_drift");
    global_home(&home);
    let rt = runtime(&home, &[]);
    let id = shared_deployment(&rt).id;
    let outcome = turn_off_for_agent(&rt, &ctx(), &request(id, "codex")).unwrap();
    let edited = home.join(".claude/skills/gamma/SKILL.md");
    std::fs::write(&edited, b"edited\n").unwrap();
    let restore = |force| RestoreRequest {
        event_id: outcome.event_id.clone(),
        force,
    };

    let refused = ops::restore_event(&rt, &ctx(), &restore(false)).unwrap_err();

    assert_eq!(
        refused.code,
        skill_studio_core::error::ErrorCode::DriftConflict
    );
    assert_eq!(std::fs::read(&edited).unwrap(), b"edited\n");
    ops::restore_event(&rt, &ctx(), &restore(true)).unwrap();
    assert!(is_link(&home.join(".claude/skills/gamma")));

    std::fs::remove_dir_all(&home).ok();
}

fn write_dotagents_ledger(home: &Path) {
    let agents_dir = home.join(".agents");
    std::fs::write(
        agents_dir.join("agents.lock"),
        "[skills.gamma]\nsource = \"owner/gamma\"\n",
    )
    .unwrap();
    std::fs::write(
        agents_dir.join("agents.toml"),
        "[[skills]]\nname = \"gamma\"\n",
    )
    .unwrap();
}

/// Flow: dotagents manages `gamma`; turn it off for Codex. Expect a refusal
/// that names "Off everywhere", the check saying the same with the way out
/// offered, and nothing written (shared folder and links intact, no copy, no
/// journal row). Catches a change that dotagents `install` would undo.
#[test]
fn a_dotagents_skill_is_refused_with_off_everywhere_and_nothing_is_written() {
    let home = unique_temp_dir("agent_off_dotagents");
    global_home(&home);
    write_dotagents_ledger(&home);
    let rt = runtime(&home, &[]);
    let id = shared_deployment(&rt).id;

    let error = turn_off_for_agent(&rt, &ctx(), &request(id.clone(), "codex")).unwrap_err();
    let check = turn_off_check(&rt, &ctx(), &request(id, "codex")).unwrap();

    assert!(
        error.message.contains("Off everywhere"),
        "{}",
        error.message
    );
    let refusal = check.refusal.unwrap();
    assert!(refusal.off_everywhere);
    assert_eq!(refusal.reason, error.message);
    assert_nothing_written(&home);

    std::fs::remove_dir_all(&home).ok();
}

fn assert_nothing_written(home: &Path) {
    assert_eq!(
        std::fs::read(home.join(".agents/skills/gamma/SKILL.md")).unwrap(),
        SKILL_MD
    );
    for link in [".claude/skills/gamma", ".pi/agent/skills/gamma"] {
        assert!(is_link(&home.join(link)), "{link} must stay a link");
    }
    assert!(std::fs::symlink_metadata(home.join(".codex/skills/gamma")).is_err());
    assert!(std::fs::symlink_metadata(home.join(".agents/skills-parked")).is_err());
    assert!(
        !home.join(".history/events.sqlite3").exists() || {
            let rt = runtime(home, &[]);
            kinds(&rt).is_empty()
        }
    );
}

/// Flow: `~/.claude/skills` is a link to the whole shared folder; turn
/// `gamma` off for Codex. Expect a refusal that offers "Off everywhere" and
/// nothing written. Catches a split that would put Claude Code's copy inside
/// the folder it is splitting.
#[test]
fn a_whole_folder_claude_link_is_refused_with_off_everywhere_and_nothing_is_written() {
    let home = unique_temp_dir("agent_off_whole_link");
    shared_skill(
        &home.join(".agents/skills"),
        &[home.join(".pi/agent/skills")],
    );
    std::os::unix::fs::symlink(home.join(".agents/skills"), home.join(".claude/skills"))
        .unwrap_or_else(|_| {
            std::fs::create_dir_all(home.join(".claude")).unwrap();
            std::os::unix::fs::symlink(home.join(".agents/skills"), home.join(".claude/skills"))
                .unwrap();
        });
    let rt = runtime(&home, &[]);
    let id = shared_deployment(&rt).id;

    let error = turn_off_for_agent(&rt, &ctx(), &request(id.clone(), "codex")).unwrap_err();
    let check = turn_off_check(&rt, &ctx(), &request(id, "codex")).unwrap();

    assert!(
        error.message.contains("link to the shared folder"),
        "{}",
        error.message
    );
    assert!(check.refusal.unwrap().off_everywhere);
    assert_eq!(
        std::fs::read(home.join(".agents/skills/gamma/SKILL.md")).unwrap(),
        SKILL_MD
    );
    assert!(std::fs::symlink_metadata(home.join(".codex/skills/gamma")).is_err());
    assert!(is_link(&home.join(".claude/skills")));
    assert!(kinds(&rt).is_empty());

    std::fs::remove_dir_all(&home).ok();
}

/// Flow: an agent that does not read the shared folder (Claude Code has no
/// link and no copy). Expect a refusal with no "Off everywhere" and nothing
/// written. Catches a "turn off" that parks a copy the agent never had.
#[test]
fn an_agent_that_does_not_read_the_shared_folder_is_refused() {
    let home = unique_temp_dir("agent_off_not_reader");
    shared_skill(
        &home.join(".agents/skills"),
        &[home.join(".pi/agent/skills")],
    );
    let rt = runtime(&home, &[]);
    let id = shared_deployment(&rt).id;

    let error = turn_off_for_agent(&rt, &ctx(), &request(id, "claude-code")).unwrap_err();

    assert!(
        error.message.contains("does not read the shared folder"),
        "{}",
        error.message
    );
    assert!(std::fs::symlink_metadata(home.join(".claude/skills/gamma")).is_err());
    assert!(kinds(&rt).is_empty());

    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a regular file blocks the `.origin` folder the parked copy's note
/// goes in, so the park step fails after the split wrote every copy. Expect
/// the error, the shared folder and links back as they were, no per-agent
/// copy, no restore row, and the split row failed with no Undo. Catches a failure that
/// leaves the user with copies for every agent and a shared folder that is
/// gone.
#[test]
fn a_park_step_that_fails_rolls_the_split_back() {
    let home = unique_temp_dir("agent_off_rollback");
    global_home(&home);
    let before = tree(&home.join(".agents/skills/gamma"));
    let parked_codex = home.join(".agents/skills-parked/codex");
    std::fs::create_dir_all(&parked_codex).unwrap();
    std::fs::write(parked_codex.join(".origin"), b"").unwrap();
    let rt = runtime(&home, &[]);
    let id = shared_deployment(&rt).id;

    let result = turn_off_for_agent(&rt, &ctx(), &request(id, "codex"));

    assert!(result.is_err(), "the park step must fail: {result:?}");
    assert_eq!(tree(&home.join(".agents/skills/gamma")), before);
    for link in [".claude/skills/gamma", ".pi/agent/skills/gamma"] {
        assert!(is_link(&home.join(link)), "{link} must be a link again");
    }
    assert!(std::fs::symlink_metadata(home.join(".codex/skills/gamma")).is_err());
    assert!(std::fs::symlink_metadata(parked_codex.join("gamma")).is_err());
    let events = ops::list_events(&rt, &ctx(), &ListEventsRequest::default()).unwrap();
    assert!(
        events.iter().all(|e| e.kind != "restore"),
        "the rollback happens inside the op and writes no restore row to undo"
    );
    let split_row = events.iter().find(|e| e.kind == "split").unwrap();
    assert_eq!(split_row.status, "failed");
    assert_eq!(split_row.restore, RestoreCapability::NoInverse);

    std::fs::remove_dir_all(&home).ok();
}

/// Flow: turn `gamma` off for Codex, then unpark the Codex copy. Expect the
/// copy back in Codex's folder, a real folder with the original bytes, read by
/// Codex again, and the parked copy gone. Catches a parked copy that Unpark
/// cannot place (the user could not turn the skill back on, which #390
/// requires).
#[test]
fn unparking_the_turned_off_copy_gives_codex_its_skill_back() {
    let home = unique_temp_dir("agent_off_unpark");
    global_home(&home);
    let rt = runtime(&home, &[]);
    let id = shared_deployment(&rt).id;
    let outcome = turn_off_for_agent(&rt, &ctx(), &request(id, "codex")).unwrap();
    let inventory = ops::scan(&rt, &ctx(), &ScanRequest::default()).unwrap();
    let parked = inventory
        .skills
        .iter()
        .flat_map(|s| s.deployments.iter())
        .find(|d| d.root.kind == RootKind::Parked && d.path == outcome.parked_path)
        .unwrap();

    ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked.id.clone(),
        },
    )
    .unwrap();

    let codex_copy = home.join(".codex/skills/gamma");
    assert!(is_real_dir(&codex_copy));
    assert_eq!(
        std::fs::read(codex_copy.join("SKILL.md")).unwrap(),
        SKILL_MD
    );
    assert!(std::fs::symlink_metadata(&outcome.parked_path).is_err());
    let inventory = ops::scan(&rt, &ctx(), &ScanRequest::default()).unwrap();
    assert!(inventory
        .skills
        .iter()
        .flat_map(|s| s.deployments.iter())
        .any(
            |d| d.root.kind == RootKind::Harness(AgentId::parse("codex").unwrap())
                && d.path == codex_copy
        ));

    std::fs::remove_dir_all(&home).ok();
}

fn refused(rt: &Runtime, home: &Path, agent: &str) -> skill_studio_core::dto::AgentOffRefusal {
    let id = shared_deployment(rt).id;
    let error = turn_off_for_agent(rt, &ctx(), &request(id.clone(), agent)).unwrap_err();
    let refusal = turn_off_check(rt, &ctx(), &request(id, agent))
        .unwrap()
        .refusal
        .unwrap_or_else(|| panic!("the check must refuse too: {}", error.message));
    assert_eq!(refusal.reason, error.message);
    assert_eq!(
        std::fs::read(home.join(".agents/skills/gamma/SKILL.md")).unwrap(),
        SKILL_MD
    );
    assert!(std::fs::symlink_metadata(home.join(".agents/skills-parked")).is_err());
    assert!(kinds(rt).is_empty());
    refusal
}

/// Flow: turn `gamma` off for Cursor. Cursor also reads the Claude Code and
/// Codex folders, and the split writes a copy into both. Expect a refusal
/// that names them and offers "Off everywhere", with nothing written. Catches
/// a "turn off" that parks Cursor's copy while Cursor still loads the others.
#[test]
fn cursor_is_refused_because_it_also_reads_the_codex_and_claude_folders() {
    let home = unique_temp_dir("agent_off_cursor");
    global_home(&home);
    let rt = runtime(&home, &[]);

    let refusal = refused(&rt, &home, "cursor");

    assert!(refusal.off_everywhere);
    assert!(refusal.reason.contains("Codex"), "{}", refusal.reason);
    assert!(refusal.reason.contains("Claude Code"), "{}", refusal.reason);
    assert!(refusal.reason.contains("Off everywhere"));
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: turn `gamma` off for `OpenCode` while Claude Code links it. `OpenCode`
/// reads `~/.claude/skills`, where the split writes Claude Code's copy.
/// Expect a refusal naming Claude Code. Catches an `OpenCode` that stays on
/// through Claude Code's copy.
#[test]
fn opencode_is_refused_when_claude_code_would_get_a_copy_it_reads() {
    let home = unique_temp_dir("agent_off_opencode_claude");
    global_home(&home);
    let rt = runtime(&home, &[]);

    let refusal = refused(&rt, &home, "open-code");

    assert!(refusal.off_everywhere);
    assert!(refusal.reason.contains("Claude Code"), "{}", refusal.reason);
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: turn `gamma` off for `OpenCode` when Claude Code has no link and no
/// copy. Expect it to go through: `OpenCode`'s copy is parked, the others live.
/// Catches a refusal that shuts out `OpenCode` when nothing else it reads holds
/// the skill.
#[test]
fn opencode_is_allowed_when_it_reads_no_other_copy() {
    let home = unique_temp_dir("agent_off_opencode_alone");
    shared_skill(
        &home.join(".agents/skills"),
        &[home.join(".pi/agent/skills")],
    );
    let rt = runtime(&home, &[]);
    let id = shared_deployment(&rt).id;

    let outcome = turn_off_for_agent(&rt, &ctx(), &request(id, "open-code")).unwrap();

    assert_eq!(
        std::fs::read(outcome.parked_path.join("SKILL.md")).unwrap(),
        SKILL_MD
    );
    assert!(std::fs::symlink_metadata(home.join(".config/opencode/skills/gamma")).is_err());
    assert!(std::fs::symlink_metadata(home.join(".claude/skills/gamma")).is_err());
    for live in [".codex/skills/gamma", ".pi/agent/skills/gamma"] {
        assert!(is_real_dir(&home.join(live)), "{live} must be a real copy");
    }
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: Codex's own settings already turn `gamma` off (by path), then turn it
/// off for pi. The split would give Codex a copy at a new path, which the
/// settings no longer cover. Expect a refusal naming Codex with "Off
/// everywhere", nothing written. Catches a turn-off for one agent that
/// switches the skill back on for another.
#[test]
fn a_reader_with_the_skill_off_in_its_own_settings_blocks_the_split() {
    let home = unique_temp_dir("agent_off_config_off");
    global_home(&home);
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    std::fs::write(
        home.join(".codex/config.toml"),
        format!(
            "[[skills.config]]\npath = \"{}\"\nenabled = false\n",
            home.join(".agents/skills/gamma/SKILL.md").display()
        ),
    )
    .unwrap();
    let rt = runtime(&home, &[]);

    let refusal = refused(&rt, &home, "pi");

    assert!(refusal.off_everywhere);
    assert!(refusal.reason.contains("Codex"), "{}", refusal.reason);
    assert!(
        refusal.reason.contains("own settings"),
        "{}",
        refusal.reason
    );
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a parked copy of `gamma` from Codex already sits in the slot. Turn
/// `gamma` off for Codex. Expect a refusal that says so and nothing written.
/// Catches a turn-off that splits first and fails at the park step.
#[test]
fn a_parked_copy_already_in_the_slot_is_refused_before_any_write() {
    let home = unique_temp_dir("agent_off_slot_taken");
    global_home(&home);
    let taken = home.join(".agents/skills-parked/codex/gamma");
    std::fs::create_dir_all(&taken).unwrap();
    std::fs::write(taken.join("SKILL.md"), SKILL_MD).unwrap();
    let rt = runtime(&home, &[]);
    let id = shared_deployment(&rt).id;

    let error = turn_off_for_agent(&rt, &ctx(), &request(id, "codex")).unwrap_err();

    assert!(
        error.message.contains("already exists"),
        "{}",
        error.message
    );
    assert!(is_link(&home.join(".claude/skills/gamma")));
    assert!(std::fs::symlink_metadata(home.join(".codex/skills/gamma")).is_err());
    assert_eq!(std::fs::read(taken.join("SKILL.md")).unwrap(), SKILL_MD);
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: the shared folder is the project's `.agents/skills`, linked from
/// the project's Claude Code folder. Turn it off for Codex, then undo.
/// Expect the Codex copy parked under the project's key, Claude Code's copy
/// live, the check naming the project, and an undo that restores the project
/// folder. Catches a project run that parks under the global slot or touches
/// the home's folders.
#[test]
fn project_scope_parks_under_the_project_key_and_undo_restores_the_project_folder() {
    let home = unique_temp_dir("agent_off_project");
    let project = home.join("work/app");
    shared_skill(
        &project.join(".agents/skills"),
        &[project.join(".claude/skills")],
    );
    let before = tree(&project.join(".agents/skills/gamma"));
    let rt = runtime(&home, std::slice::from_ref(&project));
    let id = shared_deployment(&rt).id;

    let check = turn_off_check(&rt, &ctx(), &request(id.clone(), "codex")).unwrap();
    assert!(check.refusal.is_none(), "{:?}", check.refusal);
    assert_eq!(check.project.as_deref(), Some(project.as_path()));
    let outcome = turn_off_for_agent(&rt, &ctx(), &request(id, "codex")).unwrap();

    assert!(outcome
        .parked_path
        .starts_with(home.join(".agents/skills-parked/projects")));
    assert!(outcome.parked_path.ends_with("codex/gamma"));
    assert_eq!(
        std::fs::read(outcome.parked_path.join("SKILL.md")).unwrap(),
        SKILL_MD
    );
    assert!(is_real_dir(&project.join(".claude/skills/gamma")));
    assert!(std::fs::symlink_metadata(project.join(".codex/skills/gamma")).is_err());
    assert!(std::fs::symlink_metadata(project.join(".agents/skills/gamma")).is_err());

    ops::restore_event(
        &rt,
        &ctx(),
        &RestoreRequest {
            event_id: outcome.event_id,
            force: false,
        },
    )
    .unwrap();

    assert_eq!(tree(&project.join(".agents/skills/gamma")), before);
    assert!(is_link(&project.join(".claude/skills/gamma")));
    assert!(std::fs::symlink_metadata(&outcome.parked_path).is_err());

    std::fs::remove_dir_all(&home).ok();
}
