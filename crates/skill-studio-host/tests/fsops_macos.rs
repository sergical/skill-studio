#![cfg(target_os = "macos")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use skill_studio_core::ports::ScopeFs;
use skill_studio_host::RealFs;

#[test]
fn exchange_on_macos_uses_renamex_np_or_names_the_fallback_it_took() {
    let root = tempfile::tempdir().unwrap();
    let left = root.path().join("left");
    let right = root.path().join("right");
    std::fs::create_dir(&left).unwrap();
    std::fs::create_dir(&right).unwrap();
    let left_bytes = b"left tree";
    let right_bytes = b"right tree";
    std::fs::write(left.join("marker"), left_bytes).unwrap();
    std::fs::write(right.join("marker"), right_bytes).unwrap();
    let fs = RealFs::new();

    fs.fsops_exchange(&left, &right).unwrap();

    assert_eq!(std::fs::read(left.join("marker")).unwrap(), right_bytes);
    assert_eq!(std::fs::read(right.join("marker")).unwrap(), left_bytes);

    let parent = root.path().join("parent");
    let child = parent.join("child");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::write(parent.join("marker"), left_bytes).unwrap();
    std::fs::write(child.join("marker"), right_bytes).unwrap();

    assert!(
        fs.fsops_exchange(&parent, &child).is_err(),
        "an ancestor cannot be exchanged with its descendant"
    );

    let parent_after = std::fs::read(parent.join("marker")).expect(
        "the failed swap moved the ancestor: the three-rename fallback ran instead of renamex_np",
    );
    assert_eq!(parent_after, left_bytes);
    assert_eq!(std::fs::read(child.join("marker")).unwrap(), right_bytes);
}
