// Copyright 2024 ihciah. All Rights Reserved.

//! End-to-end test for the CLI binary itself (main.rs): parse argv and run
//! the full generation pipeline on an example fixture.

use std::path::PathBuf;

#[test]
fn cli_generates_go_file() {
    let dir = std::env::temp_dir().join(format!("rust2go-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dst = dir.join("gen.go");
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/shared/user_cgo.rs")
        .to_string_lossy()
        .into_owned();
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_rust2go-cli"))
        .arg("--src")
        .arg(src)
        .arg("--dst")
        .arg(&dst)
        .arg("--no-fmt")
        .status()
        .unwrap();
    assert!(status.success());
    let go = std::fs::read_to_string(&dst).unwrap();
    assert!(go.contains("package main\n"), "unexpected output: {go}");
}
