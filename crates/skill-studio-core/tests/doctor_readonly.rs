#![allow(clippy::unwrap_used, clippy::expect_used)]
use skill_studio_core::{
    dto::DoctorRequest,
    harness::HarnessCatalog,
    identity::CorrelationId,
    ops,
    ports::{acquire_exclusive, OpContext, Runtime},
    RuntimeScope,
};
use std::sync::Arc;

fn complete_tree(
    root: &std::path::Path,
) -> std::collections::BTreeMap<std::path::PathBuf, (u64, Vec<u8>)> {
    fn walk(
        root: &std::path::Path,
        path: &std::path::Path,
        out: &mut std::collections::BTreeMap<std::path::PathBuf, (u64, Vec<u8>)>,
    ) {
        let facts = std::fs::symlink_metadata(path).unwrap();
        let bytes = if facts.is_file() {
            std::fs::read(path).unwrap()
        } else if facts.file_type().is_symlink() {
            std::fs::read_link(path)
                .unwrap()
                .as_os_str()
                .as_encoded_bytes()
                .to_vec()
        } else {
            Vec::new()
        };
        out.insert(
            path.strip_prefix(root).unwrap().to_path_buf(),
            (facts.len(), bytes),
        );
        if facts.is_dir() {
            for entry in std::fs::read_dir(path).unwrap() {
                walk(root, &entry.unwrap().path(), out);
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn doctor_core_preserves_complete_empty_and_populated_trees_or_creates_and_truncates_leases() {
    for populated in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir(&home).unwrap();
        let scope = RuntimeScope::fixture(&home);
        let leases = home.join(".history/leases");
        let ports = skill_studio_host::default_ports_with_history(
            leases,
            Arc::new(HarnessCatalog::builtin()),
            scope.history_root.join("events.sqlite3"),
        );
        let rt = Runtime::new(&scope, ports).unwrap();
        if populated {
            std::fs::create_dir_all(home.join(".agents/skills/alpha")).unwrap();
            std::fs::write(
                home.join(".agents/skills/alpha/SKILL.md"),
                "---\nname: alpha\ndescription: fixture\n---\n",
            )
            .unwrap();
            let guard = acquire_exclusive(rt.ports.leases.as_ref(), &rt.scope).unwrap();
            drop(guard);
        }
        let before = complete_tree(dir.path());
        ops::doctor(
            &rt,
            &OpContext::uncancellable(CorrelationId("readonly".into())),
            &DoctorRequest {},
        )
        .unwrap();
        assert_eq!(before, complete_tree(dir.path()));
    }
}
