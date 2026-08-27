# Rust2Go

[![Crates.io](https://img.shields.io/crates/v/rust2go.svg)](https://crates.io/crates/rust2go)
[![codecov](https://codecov.io/github/ihciah/rust2go/graph/badge.svg?token=QIID1C79QI)](https://codecov.io/github/ihciah/rust2go)

Rust2Go is a project that provides users with a simple and efficient way to call Golang from Rust with native async support. It also support user calling Rust from Golang.

## Features

- Sync and async calls from Rust to Golang
- Sync calls from Golang to Rust
- Efficient data exchange: no serialization or  socket communication, but FFI
- Simple interface design: no new invented IDL except for native rust

## How to Use

1. Define the structs and calling interfaces in restricted Rust syntax, and include generated code in the same file.
2. Generate golang code with `rust2go-cli --src src/user.rs --dst go/gen.go`
   - Use `--package-name <name>` to set the package name of the generated go file (defaults to `main`).
   - Use `--without-main` to omit the go main function, `--go118` for Go 1.18/1.19 compatibility, and `--no-fmt` to skip formatting the generated file.
3. Write a `build.rs` for you project.
4. You can then use generated implementation to call golang in your Rust project!

For detailed example, please checkout [the example projects](./examples).

### Binding File Notes

- `Option<T>` is treated as `Vec<T>`: `None` maps to an empty list on the Go side.
- Non-generic type aliases (e.g. `pub type Amount = i64;`) can be used in struct fields and trait signatures; they are expanded during code generation.
- Structs keep their own attribute macros (e.g. `#[derive(...)]`) in the generated code, and `#[rust2go::r2g_struct_tag(json = "snake_case")]` adds tags to the generated Go struct fields. See [docs/trait-attrs.md](./docs/trait-attrs.md) for the full attribute reference.

## Key Design

> Detailed design details can be found in this article: [Design and Implementation of a Rust-Go FFI Framework](https://en.ihcblog.com/rust2go/).

### Why Fast?

1. Memory layout: Rust2go only manipulates memory when needed. In most cases it passes memory reference.
2. Message passing: Rust2go relies on CGO to pass calling information. In addition, it also supports lock-free queues based on shared memory to improve performance during high-frequency communication.
3. Other optimizations: Rust2go uses Go callback based on manual assembly instead of CGO to achieve better performance.

In order to achieve the ultimate performance, this project is not purely based on communication, but on FFI to pass specially encoded data. In order to reduce memory operations to a minimum, data that satisfies a specific memory layout is passed directly by reference rather than copied.

For example, `Vec<u8>` and `String` is represented as a pointer and a length. However, structs like `Vec<String>` or `Vec<Vec<u8>>` require intermediate representation. In order to reduce the number of memory allocations to one, I use a precomputed size buffer to store these intermediate structures.

### Memory Safety

On the Golang side, the data it receives is referenced from Rust. The Rust side will do its best to ensure the validity of this data during the call. So the Golang side can implement the handler arbitrarily, but manually deep copy when leaking data outside the function life cycle.

On the Rust side, it is needed to ensure that the slot pointer of the callback ffi operation, and the user parameters are valid when the future drops. This is archieved by implementing an atomic slot structure and providing a `[drop_safe]` attribute to require user passing parameters with ownership.

Note: Since golang may scan the stack, and when it meets peer pointer, it may panic. You should run the program with `GODEBUG=invalidptr=0,cgocheck=0` env to bypass it.

## Toolchain Requirements

- Golang: >=1.18
  - For >=1.18 && < 1.20: generate golang code with `--go118`
  - For >=1.20: generate golang code normally
- Rust: >=1.75 if you want to use async

## Platform Support

- Linux, macOS and Windows are supported.
- The ASM-based callback is available on amd64 and arm64; on other platforms it falls back to the CGO implementation automatically.
- The shared memory based implementation (`#[mem]`/`#[shm]`) requires unix.

## Milestones

### Init Version

- [x] IDL(in rust) parse
- [x] Go code generation
- [x] Build script helper
- [x] Basic data types and convertion generation
- [x] Rust impl generation
- [x] Future and basic synchronization primitives used

### Basic Ability Enhancement

- [x] More complicated data types support
- [x] Support user passing references
- [x] More elegant code generation implementation
- [x] Better build cache control
- [x] Golang interface support(separate user code from generated code)
- [x] Dynamic linking support
- [x] Golang helper library

### Performance Optimization

- [x] Shared memory based implementation
- [x] Faster ASM-based callback instead of CGO

### Extended Features

- [x] Support calling rust from golang

## Coverage

[![codecov sunburst](https://codecov.io/github/ihciah/rust2go/graphs/sunburst.svg?token=QIID1C79QI)](https://codecov.io/github/ihciah/rust2go)

## Credit

This project is inspired by [fcplug](https://github.com/andeya/fcplug).
