// Copyright 2024 ihciah. All Rights Reserved.

//! End-to-end tests for `generate()`: run the full codegen pipeline on the
//! example fixtures and assert on the generated Go bindings. These tests are
//! the only coverage of this crate — in real builds `generate()` is invoked
//! from build scripts, which coverage instrumentation does not observe.

use std::path::PathBuf;

use rust2go_gen::{generate, GenArgs};

fn fixture(rel: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(rel)
        .to_string_lossy()
        .into_owned()
}

fn run_generate(case: &str, src: &str, args: impl FnOnce(&mut GenArgs)) -> String {
    let dir =
        std::env::temp_dir().join(format!("rust2go-gen-test-{}-{}", std::process::id(), case));
    std::fs::create_dir_all(&dir).unwrap();
    let dst = dir.join("gen.go");
    let mut gen_args = GenArgs {
        src: fixture(src),
        dst: dst.to_string_lossy().into_owned(),
        // Never shell out to `go fmt` unless a test explicitly asks for it.
        no_fmt: true,
        ..Default::default()
    };
    args(&mut gen_args);
    generate(&gen_args);
    std::fs::read_to_string(&dst).unwrap()
}

#[test]
fn cgo_go118_default_package() {
    let go = run_generate("cgo-118", "examples/shared/user_cgo.rs", |a| {
        a.go118 = true;
    });
    // Default package name is "main" and the main func is emitted.
    assert!(
        go.contains("package main\n"),
        "missing package clause: {go}"
    );
    assert!(go.contains("func main() {}"), "missing main func: {go}");
    assert!(go.contains("import \"C\""), "missing cgo import: {go}");
    // go118 uses the reflect-based string helpers.
    assert!(go.contains("\"reflect\""), "missing reflect import: {go}");
    // demo_sum is a #[cgo_callback], the other calls use asmcall.
    assert!(
        go.contains("\"github.com/ihciah/rust2go/cgocall\""),
        "missing cgocall import: {go}"
    );
    assert!(
        go.contains("\"github.com/ihciah/rust2go/asmcall\""),
        "missing asmcall import: {go}"
    );
    // Non-mem calls need the runtime import for finalizer-based drop.
    assert!(go.contains("\"runtime\""), "missing runtime import: {go}");
    // cbindgen emitted the ref structs into the C preamble.
    assert!(go.contains("DemoUserRef"), "missing ref structs: {go}");
}

#[test]
fn cgo_latest_custom_package_without_main() {
    let go = run_generate("cgo-latest", "examples/shared/user_cgo.rs", |a| {
        a.package_name = "custom".into();
        a.without_main = true;
    });
    assert!(
        go.contains("package custom\n"),
        "missing package clause: {go}"
    );
    assert!(!go.contains("func main()"), "unexpected main func: {go}");
    assert!(
        !go.contains("\"reflect\""),
        "reflect import should be go118-only: {go}"
    );
}

#[test]
fn mem_uses_shm_imports() {
    let go = run_generate("mem", "examples/shared/user_mem.rs", |_| {});
    // The #[mem] attribute switches the backend to shared memory.
    assert!(
        go.contains("mem_ring \"github.com/ihciah/rust2go/mem-ring\""),
        "missing mem-ring import: {go}"
    );
    assert!(
        go.contains("\"github.com/panjf2000/ants/v2\""),
        "missing ants import: {go}"
    );
    assert!(
        go.contains("typedef struct QueueMeta {"),
        "missing shm C include: {go}"
    );
    assert!(go.contains("func ringsInit("), "missing ringsInit: {go}");
    // All calls are mem calls, so neither asmcall nor the runtime-based
    // drop machinery is needed.
    assert!(
        !go.contains("\"github.com/ihciah/rust2go/asmcall\""),
        "unexpected asmcall import: {go}"
    );
    assert!(
        !go.contains("\"runtime\""),
        "unexpected runtime import: {go}"
    );
}

#[test]
fn g2r_emits_drop_and_exports() {
    let go = run_generate(
        "g2r",
        "examples/example-go2rust/rust-lib/src/user.rs",
        |_| {},
    );
    // demo_convert_name returns a String, so the internal drop helper and
    // the runtime import are required.
    assert!(
        go.contains("c_rust2go_internal_drop"),
        "missing internal drop: {go}"
    );
    assert!(go.contains("\"runtime\""), "missing runtime import: {go}");
    // g2r calls default to asmcall.
    assert!(
        go.contains("\"github.com/ihciah/rust2go/asmcall\""),
        "missing asmcall import: {go}"
    );
    assert!(go.contains("demo_log"), "missing g2r fn: {go}");
    assert!(go.contains("demo_convert_name"), "missing g2r fn: {go}");
}

#[test]
fn go_fmt_branch() {
    // Exercises the `go fmt` subprocess branch. CI always installs Go; on a
    // machine without Go this test panics, which matches how build scripts
    // behave anyway.
    let go = run_generate("fmt", "examples/shared/user_cgo.rs", |a| {
        a.no_fmt = false;
    });
    assert!(
        go.contains("package main\n"),
        "missing package clause: {go}"
    );
}
