//! Regression tests for the "outside the tarball" guards in
//! `locate_extracted_skill_dir` and `find_skill_dir_by_name`. A fetched
//! tarball is untrusted: a `..` path or a link inside it must never let a
//! fork copy a folder from outside the extraction. Each case here resolves
//! to a folder that really exists outside, so only the guard can refuse it
//! (a missing target would fail earlier with "not found").

use std::fs;
use std::path::Path;

use super::locate_extracted_skill_dir;

const REFUSED: &str = "Refusing to extract a path outside the tarball";

fn write_skill(dir: &Path, name: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: d\n---\n"),
    )
    .unwrap();
}

/// `<tmp>/extract/<top>/` plus a real skill folder outside the extraction
/// at `<tmp>/outside/skills/find-bugs`.
fn layout() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let extract = tmp.path().join("extract");
    let outside = tmp.path().join("outside");
    write_skill(&outside.join("skills/find-bugs"), "find-bugs");
    (tmp, extract, outside)
}

#[test]
fn a_dot_dot_path_to_an_existing_folder_outside_is_refused() {
    let (_tmp, extract, _outside) = layout();
    write_skill(&extract.join("owner-repo-abc/skills/other"), "other");

    let error = locate_extracted_skill_dir(&extract, "../../outside/skills/find-bugs").unwrap_err();
    assert_eq!(error, REFUSED);
}

#[test]
fn a_link_inside_the_tarball_to_a_folder_outside_is_refused() {
    let (_tmp, extract, outside) = layout();
    let top = extract.join("owner-repo-abc");
    fs::create_dir_all(top.join("skills")).unwrap();
    std::os::unix::fs::symlink(
        outside.join("skills/find-bugs"),
        top.join("skills/find-bugs"),
    )
    .unwrap();

    let error = locate_extracted_skill_dir(&extract, "skills/find-bugs").unwrap_err();
    assert_eq!(error, REFUSED);
}

#[test]
fn a_by_name_match_reached_through_a_linked_top_folder_is_refused() {
    let (_tmp, extract, outside) = layout();
    fs::create_dir_all(&extract).unwrap();
    // The tarball's only top-level entry is a link out of the extraction,
    // so the by-name search walks folders outside it.
    std::os::unix::fs::symlink(&outside, extract.join("owner-repo-abc")).unwrap();

    let error = locate_extracted_skill_dir(&extract, "find-bugs").unwrap_err();
    assert_eq!(error, REFUSED);
}
