# Changelog

All notable changes since 2589bc4 (`feat: add CustomArgGoCompiler to support custom go build args (#68)`, 2025-04) are documented here, newest first. This project does not use git tags; the log below is the release record.

## 2026-08-27

### Fixed
- Correct the third argument name in `CallFunc{G0,}P3` assembly (`arg1+0x18(FP)` → `arg2+0x18(FP)` on amd64 and arm64; behavior unchanged, fixes `go vet` asmdecl). (#179)
- Replace the bogus `#[mem_call]` attribute with `#[mem]` on `multi_param_test` in the test crate — the unknown attribute was silently stripped, so the test never exercised the shared-memory path. It now runs as a real shared-memory call. (#177)

### Changed
- `rust2go-mem-ffi` and `mem-ring`: the `monoio` and `tokio` cargo features are now **mutually exclusive** and fail with `compile_error!` when combined. Previously the combination silently selected the monoio internals while dependents observed tokio as enabled. (#178)
- CI: workspace-wide commands no longer use `--all-features` (the two `-mem` examples enable opposite runtimes); the tokio branches of `rust2go-mem-ffi` and `example-tokio-mem` are now built, tested, and linted in dedicated steps — they were previously never compiled in CI. See `docs/ci.md`. (#178)

### Documentation
- New `docs/ci.md` describing the CI pipeline, the gen.go freshness check, and the feature-matrix split. (#178)
- `mem-ring/README.md` documents the `monoio`/`tokio` mutual exclusion. (#178)

## 2026-08-26

### Added
- Safety-net test coverage: assertion-based golden tests for the `rust2go-common` Go emitters (previously a println-only smoke test), unit tests for `rust2go-mem-ffi` (Payload/flag protocol/TaskDesc/slab helpers/LocalFut), Go unit tests for `mem-ring` (Slab/MultiSlab/ring) and `asmcall` (cgo correctness tests in the standalone `asmcall/calltest` package). (#176)
- CI: `go test ./asmcall/... ./mem-ring/...` step, and a generated-file freshness check (`git diff --exit-code -- '**/gen.go'`) that fails the build if the committed Go bindings drift from the `.rs` sources. (#176)

### Fixed
- `mem-ring`: the Slab freelist used `0` as both the empty sentinel and valid slot index 0 (slot 0 was never reused); `Pop` now clears the stored reference so the GC can reclaim it, and is bounds-checked. (#176)
- `mem-ring`: the Write drainer goroutine self-deadlocked when `continue` jumped back to `Lock()` while holding the mutex. (#176)

## 2026-08-24

### Added
- Code coverage collection (cargo-llvm-cov + Go coverprofile) uploaded to Codecov, with a README badge and sunburst graph. (#173, #175)
- Unit tests for the core crates (`rust2go` slot/future, `rust2go-convert`). (#173)

### Fixed
- Support type aliases in binding files. (#172, #174)
- `mem-ring` fd and heap safety issues (socketpair/eventfd handling and teardown heap corruption). (#173)

## 2026-06 ~ 2026-08

### Added
- Support for a custom package name in the generated Go file. (#152)

### Changed
- Dependabot updates are grouped into monthly PRs. (#168)
- bindgen updated to 0.72; assorted dependency bumps (bytes, slab, sccache-action, actions/checkout). (#150, #146, #141, #157, #170, #117, #137)

## 2026-02

### Added
- `Option<T>` is now supported and treated as `Vec<T>` (empty = `None`). (#101)
- The macro preserves attribute macros (e.g. `serde` derives) on structs. (#122)
- `go118` builder toggle for generating Go 1.18-compatible bindings; committed bindings regenerated.

### Fixed
- Avoid dangling pointers for empty `Option`/list values passed to Go. (#93, #132, 83e6692)
- Reset `libgo.h` atime/mtime so the Rust-side build cache is used correctly. (#90)

## 2025-04 ~ 2025-11

### Fixed
- Compilation on Windows. (#75)
- Apply the address operator to all parameters in `mem_call` functions. (#89)
- Prevent passing `NonNull::dangling()` to Go (incl. `Vec<T>` with complex element types). (#93, #132)
- Assorted lint/dead-code warning fixes and README corrections for the `rust2go-cli` command syntax. (#97, #111, #110)

### Added
- Expanded test suite and boundary tests. (#69)
