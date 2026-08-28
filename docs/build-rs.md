---
title: Build script helper
date: 2026-08-27
author: lirenjie95
---

# Build Script Helper

The `build` feature of the `rust2go` crate provides a `Builder` that compiles your Go code and generates the Rust FFI bindings as part of `cargo build`.

```toml
[build-dependencies]
rust2go = { version = "0.4", features = ["build"] }
```

## Minimal setup

```rust
// build.rs
fn main() {
    rust2go::Builder::new().with_go_src("./go").build();
}
```

This runs `go build -buildmode=c-archive` in `./go`, generates `_go_bindings.rs` into `OUT_DIR` with bindgen, and links the resulting static library. Include the bindings in your crate with:

```rust
pub mod binding {
    rust2go::r2g_include_binding!();
}
```

## Regenerating Go code in the build script

`with_regen(src, dst)` (or `with_regen_arg(RegenArgs { .. })` for full control) runs the rust2go code generator before building, so the generated Go file always matches the Rust source:

```rust
use rust2go::RegenArgs;

fn main() {
    rust2go::Builder::new()
        .with_go_src("./go")
        .with_regen_arg(RegenArgs {
            src: "./src/user.rs".into(),
            dst: "./go/gen.go".into(),
            ..Default::default()
        })
        .build();
}
```

`RegenArgs` mirrors the `rust2go-cli` flags: `package_name`, `without_main`, `go118` and `no_fmt`.

## Dynamic linking

Static linking is the default. To link dynamically, and optionally copy the shared library next to the build output or to a custom directory:

```rust
fn main() {
    rust2go::Builder::new()
        .with_go_src("./go")
        .with_link(rust2go::LinkType::Dynamic)
        .with_copy_lib(rust2go::CopyLib::DefaultPath) // or CopyLib::CustomPath(dir)
        .build();
}
```

The library file name follows the platform convention: `libgo.a`/`go.lib` for static linking, `libgo.so`/`libgo.dylib`/`go.dll` for dynamic linking.

Note that `with_copy_lib` only copies the library file; it does not configure the runtime loader. `cargo run` works because cargo sets the library search path for you, but when you execute the binary directly (or deploy it), the dynamic linker must be able to find the library:

- Linux: set `LD_LIBRARY_PATH`, or install the library into a system library directory.
- macOS: set `DYLD_LIBRARY_PATH`, or link with an rpath (e.g. `RUSTFLAGS="-C link-args=-Wl,-rpath,<dir>"`).
- Windows: put `go.dll` next to the executable or on `PATH`.

## Custom binding file name

`with_binding("my_bindings.rs")` changes the generated Rust binding file name (default `_go_bindings.rs`). Include it with `rust2go::r2g_include_binding!("my_bindings.rs")`.

## Customizing the Go build

The default compiler is `CustomArgGoCompiler`, which accepts extra `go build` arguments and environment variables:

```rust
fn main() {
    let mut builder = rust2go::Builder::new().with_go_src("./go");
    builder
        .compiler_arg("-tags=mycustomtag")
        .compiler_env("CGO_ENABLED", "1");
    builder.build();
}
```

For full control over the build, implement the `GoCompiler` trait and plug it in with `with_go_compiler(...)`. `DefaultGoCompiler` is the plain implementation without extra arguments.

## Notes

- On Windows the generated header is named `go.h` and the static library has the `.lib` extension; this is handled automatically.
- When the generated C header is unchanged between builds, the helper restores its atime/mtime so that dependent crates are not recompiled unnecessarily.
- Crates using rust2go may use Rust edition 2021 or 2024 (edition 2024 requires Rust >=1.85).
