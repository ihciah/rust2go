---
title: CI Pipeline
date: 2026-08-24
author: lirenjie95
---

# CI Pipeline

The GitHub Actions workflow (`.github/workflows/ci.yml`) runs three jobs on every PR and on pushes to `master`. This document describes what each job covers and the non-obvious constraints behind the current shape.

## Jobs

### build-and-test

- `cargo check` / `cargo test` over the workspace, plus dedicated runs for the feature combinations that a workspace-wide build cannot cover (see "Feature matrix" below).
- `go test ./asmcall/... ./mem-ring/...` runs the Go unit tests (asmcall correctness tests live in the standalone `asmcall/calltest` package — cgo is not allowed in test files of packages that have no non-test Go files, and cgo test files inside `asmcall` itself would break dependents built with `-buildmode=c-archive`).
- **Generated-file freshness check**: `git diff --exit-code -- '**/gen.go'` runs after the workspace build. Building the `test` crate and the examples regenerates the committed `gen.go` bindings via their `build.rs` scripts; if the committed copies have drifted from what the `.rs` sources generate, the job fails. When you change anything that affects codegen, regenerate (or hand-update) the committed `gen.go` files — the CI log prints the exact diff if they diverge.
- The job sets `GODEBUG: invalidptr=0,cgocheck=0`, which the asmcall-based FFI requires; it also masks genuine pointer bugs, so do not rely on it as a safety signal.

### coverage

- `cargo llvm-cov` over the workspace (same feature-matrix exclusions as build-and-test), Go coverage from `test/go`, uploaded to Codecov.

### lint

- `cargo fmt --check` and `cargo clippy ... -- --deny warnings` over the workspace, again with the feature-matrix split below.

## Feature matrix: why there is no `--all-features`

`rust2go-mem-ffi` and `mem-ring` select their async runtime with `all(feature = "tokio", not(feature = "monoio"))` gates, and the `monoio`/`tokio` features are **mutually exclusive** — enabling both is a `compile_error!`, because the combination would silently select the monoio (`Rc<UnsafeCell>`) internals while dependents observe tokio as enabled.

The two `-mem` examples enable opposite runtimes of `rust2go-mem-ffi` (`example-monoio-mem` uses the default monoio, `example-tokio-mem` uses `default-features = false, features = ["tokio"]`). Any workspace-wide cargo invocation would therefore unify the forbidden combination, so:

- Workspace-wide commands exclude `example-tokio-mem` (and use default features elsewhere).
- `example-tokio-mem` and the tokio branch of `rust2go-mem-ffi` are checked/tested/linted in dedicated steps (`-p ... --no-default-features --features tokio`).
- `mem-ring` is tested twice: default features (monoio) and `--no-default-features --features tokio`.
- `rust2go`'s optional `build` feature (bindgen/cbindgen machinery) is checked and linted in its own step.

A side effect of the split: the tokio branches of `rust2go-mem-ffi` are actually compiled in CI now — under `--all-features` they were always cfg'd out.

## Adding new crates or features

- If a new crate introduces mutually exclusive features, add a `compile_error!` guard and extend the matrix above the same way; never reintroduce workspace-wide `--all-features`.
- If a new crate generates Go bindings committed to git, the freshness check covers it automatically as long as its `build.rs` runs during the workspace build.
