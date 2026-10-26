//! `scripts/build-gpui.sh` packages the release app bundle on macOS.
#![cfg(target_os = "macos")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

fn run(command: &mut Command) -> Output {
    let output = command.output().expect("command starts");
    assert!(
        output.status.success(),
        "{command:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn bundle_takes_the_release_name_identifier_and_crate_version() {
    let target = tempfile::tempdir().unwrap();
    let profile = target.path().join("debug");
    fs::create_dir_all(&profile).unwrap();
    let executable = profile.join("thoughttree-gpui");
    fs::write(&executable, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();

    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/build-gpui.sh");
    run(Command::new(script)
        .args(["--debug", "--no-build"])
        .env("CARGO_TARGET_DIR", target.path()));

    let bundle = profile.join("bundle/ThoughtTree.app");
    run(Command::new("codesign")
        .args(["--verify", "--strict"])
        .arg(&bundle));
    let plist = run(Command::new("plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(bundle.join("Contents/Info.plist")));
    let info: serde_json::Value = serde_json::from_slice(&plist.stdout).unwrap();
    assert_eq!(info["CFBundleName"], "ThoughtTree");
    assert_eq!(info["CFBundleDisplayName"], "ThoughtTree");
    assert_eq!(info["CFBundleIdentifier"], "com.david.thoughttree");
    assert_eq!(info["CFBundleExecutable"], "thoughttree-gpui");
    assert_eq!(
        info["CFBundleShortVersionString"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(info["CFBundleVersion"], env!("CARGO_PKG_VERSION"));
}
