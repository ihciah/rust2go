# Shared demo template

This directory holds the single copy of the demo shared by the four
Rust -> Go examples (`example-tokio`, `example-monoio`, `example-tokio-mem`,
`example-monoio-mem`):

- `user_cgo.rs` / `user_mem.rs` — structs and the `DemoCall` trait per backend
  (CGO / shared memory). Examples include them from `src/main.rs` with
  `#[path = "../../shared/user_*.rs"] mod user;`, and the build scripts point
  the codegen (`RegenArgs::src`) at the same files.
- `build_cgo.rs` / `build_mem.rs` — build script bodies, pulled into each
  example's one-line `build.rs` with `include!`.
- `impl.go.tmpl` — canonical Go implementation of the generated `DemoCall`
  interface. Each example keeps a byte-identical copy at `go/impl.go` (Go has
  no include mechanism and the implementation must be in the same package as
  the generated `gen.go`); CI diffs the copies against this template.

See [examples/README.md](../README.md) for the full walkthrough.
