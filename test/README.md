# Rust2Go Integrated Test

This directory contains an example integration test for the Rust2Go code generator. It demonstrates invoking `rust2go::Builder` in `build.rs` to generate Go bindings for Rust code defined under `src/user.rs`. The generated Go file can be found in `go/gen.go`.

## Project Structure

- `src/`: Rust library source code for integration testing (`user.rs`).
- `go/`: Generated and example Go code.
- `build.rs`: Build script that drives Rust2Go code generation.
- `Cargo.toml`: Defines dependencies and build dependencies (including `rust2go`).
- `README.md`: This file.

## Usage

1. Run `cargo build` to trigger `build.rs` and generate Go bindings.
2. Inspect the generated code under `go/gen.go`.
3. Run `cargo test` to execute integration tests.
