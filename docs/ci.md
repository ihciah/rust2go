---
title: CI Pipeline
date: 2026-08-24
author: lirenjie95
---

# CI Pipeline

The GitHub Actions workflow (`.github/workflows/ci.yml`) runs four jobs on every PR and on pushes to `master`. This document describes what each job covers and the non-obvious constraints behind the current shape.

## Jobs

### build-and-test

- `cargo check` / `cargo test` over the workspace, plus dedicated runs for the feature combinations that a workspace-wide build cannot cover (see "Feature matrix" below).
- **Generated-file freshness check**: `git diff --exit-code -- '**/gen.go'` runs after the workspace build. Building the `test` crate and the examples regenerates the committed `gen.go` bindings via their `build.rs` scripts; if the committed copies have drifted from what the `.rs` sources generate, the job fails. When you change anything that affects codegen, regenerate (or hand-update) the committed `gen.go` files — the CI log prints the exact diff if they diverge. Only `test/go/gen.go`, `examples/example-bidirectional/go/gen.go` and `examples/example-go2rust/gen.go` are committed (the last one is regenerated manually with rust2go-cli, not by a build script, so it never drifts during CI); the four r2g examples (`example-tokio`, `example-monoio`, `example-tokio-mem`, `example-monoio-mem`) gitignore their `gen.go` because it is regenerated from the shared demo in `examples/shared/` on every build.
- **Example `impl.go` sync check**: the four r2g examples share one Go demo implementation. Each keeps a byte-identical copy of `examples/shared/impl.go.tmpl` at `go/impl.go` (Go has no include mechanism and the implementation must live in the same package as the generated `gen.go`), and the job diffs every copy against the template.

### go

- Matrix job for everything Go-only, with the toolchain selected via `actions/setup-go` (the values live in the workflow-level `GO_VERSION` / `GO_MIN_VERSION` env vars and are repeated as matrix literals because matrices cannot reference `env`). `GO_VERSION` is the `stable` alias, which always resolves to the latest Go release — CI follows new Go versions automatically instead of going stale on a pinned literal; only the minimum supported toolchain is pinned:
  - **ubuntu × stable**: `gofmt -l` gate (fails on any unformatted file), `go vet` over the root module and `test/go`, and the Go unit tests (`go test ./asmcall/... ./mem-ring/...`, plus `go test` in `test/go`). Vet runs with `-unsafeptr=false` for both modules: the FFI ABIs deliberately materialize pointers from raw addresses (`mem-ring`'s `QueueMeta` carries `uintptr`s; the generated bindings convert `Payload.Ptr`), which that analyzer always flags; runtime pointer checking is already disabled via `GODEBUG`.
  - The job resets `CC`/`CXX` to empty — the workflow-level values point at sccache for the Rust jobs, and this job has no sccache installed (an empty `CC` makes cgo fall back to the per-OS default compiler).
  - **ubuntu × 1.18** (oldest supported toolchain): build + test only, over the same packages (`asmcall`, `mem-ring`, `test/go`). This is the compatibility guarantee behind the `--go118` codegen flag — `test/go/gen.go` is generated with `go118: true`, so this leg proves the reflect-based helper code path keeps compiling and passing on Go 1.18. (To make this leg possible, mem-ring's Go sources spell out the unix GOOS set in their build constraints instead of using the `unix` tag, which Go only recognizes since 1.19.)
  - **macOS × stable** (arm64): build + test + `go vet`. The `asmcall/calltest` correctness tests are gated `(linux || darwin) && (amd64 || arm64)`, so this leg is the only one that compiles and exercises the arm64 assembly path of `asmcall`.
- asmcall correctness tests live in the standalone `asmcall/calltest` package — cgo is not allowed in test files of packages that have no non-test Go files, and cgo test files inside `asmcall` itself would break dependents built with `-buildmode=c-archive`.
- The job sets `GODEBUG: invalidptr=0,cgocheck=0`, which the asmcall-based FFI requires; it also masks genuine pointer bugs, so do not rely on it as a safety signal.

### coverage

- `cargo llvm-cov` over the workspace (same feature-matrix exclusions as build-and-test), Go coverage from `test/go`, uploaded to Codecov. The Go toolchain comes from the same `GO_VERSION` (`stable`) as the go job.

### lint

- `cargo fmt --check` and `cargo clippy ... -- --deny warnings` over the workspace, again with the feature-matrix split below.

## Workflow conventions

- The sccache setup step is defined once in `build-and-test` as a YAML anchor and aliased in `lint` (`*setup-sccache`). GitHub Actions supports YAML anchors since 2025-09, but not merge keys — alias whole step maps only. (A composite action was considered and rejected: composite steps do not support `continue-on-error`, which the sccache setup needs.)

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
