package main

/*
// The g2r entry points below are provided by the Rust side (see the
// G2RCounter trait in ../src/user.rs) when this package is built as a
// c-archive and linked into a Rust binary (cargo build/test). The CI go
// job builds this package standalone without the Rust library, so weak
// no-op stubs keep the linker satisfied there. When the Rust library is
// present, its strong definitions take precedence over these weak ones,
// so the stubs are never called in real runs.

const void c_G2RCounter_incr(const void*, const void*) __attribute__((weak));
const void c_G2RCounter_incr(const void* slot, const void* params) {}

const void c_G2RCounter_current(const void*) __attribute__((weak));
const void c_G2RCounter_current(const void* slot) {}
*/
import "C"
