//! The atomic write of the app files (`docs/storage.md`, common rules).

mod support;

use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use nicecooling::app_settings::write_atomic;
use support::graphs::Dir;

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// A file `<path>.tmp` of the user is not the temporary file of the write, and the write leaves
/// no temporary file behind.
#[test]
fn a_user_file_named_like_a_temporary_file_stays() {
    let dir = Dir::new("wa-user-tmp");
    let path = dir.path().join("graphs.json");
    std::fs::write(dir.path().join("graphs.json.tmp"), "my notes").unwrap();
    write_atomic(&path, b"{}", 0o666).unwrap();
    write_atomic(&path, b"[]", 0o666).unwrap();
    assert_eq!(read(&path), "[]");
    assert_eq!(read(&dir.path().join("graphs.json.tmp")), "my notes");
    assert_eq!(names(dir.path()), ["graphs.json", "graphs.json.tmp"]);
}

#[test]
fn the_mode_is_set_at_the_creation() {
    let dir = Dir::new("wa-mode");
    let path = dir.path().join("secrets.toml");
    write_atomic(&path, b"x", 0o600).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

/// A failed write leaves the target and no temporary file.
#[test]
fn a_failed_write_removes_the_temporary_file() {
    let dir = Dir::new("wa-failed");
    let path = dir.path().join("graphs.json");
    // The rename of a file over a directory fails.
    std::fs::create_dir(&path).unwrap();
    assert!(write_atomic(&path, b"{}", 0o666).is_err());
    assert_eq!(names(dir.path()), ["graphs.json"]);
}

/// A dotfile manager links `graphs.json` to a file in its own directory. The write goes to the
/// target, the link stays a link, and no temporary file lands in the directory of the link.
#[test]
fn a_symbolic_link_stays_a_link_and_the_target_gets_the_write() {
    let dir = Dir::new("wa-link");
    let dotfiles = dir.path().join("dotfiles");
    let config = dir.path().join("config");
    std::fs::create_dir_all(&dotfiles).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    let target = dotfiles.join("graphs.json");
    std::fs::write(&target, "old").unwrap();
    let link = config.join("graphs.json");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    write_atomic(&link, b"new", 0o666).unwrap();
    assert!(link.is_symlink());
    assert_eq!(std::fs::read_link(&link).unwrap(), target);
    assert_eq!(read(&target), "new");
    assert_eq!(names(&dotfiles), ["graphs.json"]);
    assert_eq!(names(&config), ["graphs.json"]);
}

/// A relative link is read from the directory of the link, and a link to a link is followed.
#[test]
fn a_relative_link_and_a_chain_of_links_reach_the_target() {
    let dir = Dir::new("wa-relative-link");
    let dotfiles = dir.path().join("dotfiles");
    let config = dir.path().join("config");
    std::fs::create_dir_all(&dotfiles).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    let target = dotfiles.join("graphs.json");
    std::fs::write(&target, "old").unwrap();
    std::os::unix::fs::symlink("../dotfiles/graphs.json", config.join("first")).unwrap();
    std::os::unix::fs::symlink("first", config.join("second")).unwrap();

    write_atomic(&config.join("second"), b"new", 0o666).unwrap();
    assert_eq!(read(&target), "new");
    assert!(config.join("second").is_symlink() && config.join("first").is_symlink());
    assert_eq!(names(&dotfiles), ["graphs.json"]);
}
