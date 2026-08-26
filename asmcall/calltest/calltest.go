//go:build linux && (amd64 || arm64)
// +build linux
// +build amd64 arm64

// Copyright 2024 ihciah. All Rights Reserved.

// Package calltest provides cgo wrappers used by the asmcall correctness
// tests. The cgo preamble and all C symbol references must live in this
// non-test file: cgo is not supported in test-only packages (the go tool
// rejects "use of cgo in test" when a package has no non-test Go files).
package calltest

/*
#include <stdint.h>

uintptr_t p0_counter = 0;

void reset_counter(void) { p0_counter = 0; }
void bump_counter(void) { p0_counter += 1; }
uintptr_t read_counter(void) { return p0_counter; }

void inc_ptr(uintptr_t *p) { *p = *p + 1; }
void add_to(uintptr_t a, uintptr_t *p) { *p = *p + a; }
void sum_into(uintptr_t a, uintptr_t b, uintptr_t *out) { *out = a + b; }
*/
import "C"

import (
	"unsafe"

	"github.com/ihciah/rust2go/asmcall"
)

// The CallFunc* entry points return nothing, so results are observed
// through a C global (0-arg callees) or an out pointer (1..3-arg callees).

func ResetCounter() { C.reset_counter() }

func ReadCounter() uintptr { return uintptr(C.read_counter()) }

func CallG0P0Bump() { asmcall.CallFuncG0P0(C.bump_counter) }

func CallP0Bump() { asmcall.CallFuncP0(C.bump_counter) }

func CallG0P1Inc(p unsafe.Pointer) { asmcall.CallFuncG0P1(C.inc_ptr, p) }

func CallP1Inc(p unsafe.Pointer) { asmcall.CallFuncP1(C.inc_ptr, p) }

func CallG0P2AddTo(a uintptr, p unsafe.Pointer) {
	asmcall.CallFuncG0P2(C.add_to, unsafe.Pointer(a), p)
}

func CallP2AddTo(a uintptr, p unsafe.Pointer) {
	asmcall.CallFuncP2(C.add_to, unsafe.Pointer(a), p)
}

func CallG0P3SumInto(a, b uintptr, out unsafe.Pointer) {
	asmcall.CallFuncG0P3(C.sum_into, unsafe.Pointer(a), unsafe.Pointer(b), out)
}

func CallP3SumInto(a, b uintptr, out unsafe.Pointer) {
	asmcall.CallFuncP3(C.sum_into, unsafe.Pointer(a), unsafe.Pointer(b), out)
}
