//go:build linux && (amd64 || arm64)
// +build linux
// +build amd64 arm64

// Copyright 2024 ihciah. All Rights Reserved.

package asmcall

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
	"testing"
	"unsafe"
)

// The CallFunc* entry points return nothing, so results are observed
// through a C global (0-arg callees) or an out pointer (1..3-arg callees).

func TestCallFuncG0P0(t *testing.T) {
	C.reset_counter()
	CallFuncG0P0(C.bump_counter)
	CallFuncG0P0(C.bump_counter)
	if got := uintptr(C.read_counter()); got != 2 {
		t.Fatalf("counter = %d, want 2", got)
	}
}

func TestCallFuncG0P1(t *testing.T) {
	var v uintptr = 41
	CallFuncG0P1(C.inc_ptr, unsafe.Pointer(&v))
	if v != 42 {
		t.Fatalf("v = %d, want 42", v)
	}
}

func TestCallFuncG0P2(t *testing.T) {
	var v uintptr = 40
	CallFuncG0P2(C.add_to, unsafe.Pointer(uintptr(2)), unsafe.Pointer(&v))
	if v != 42 {
		t.Fatalf("v = %d, want 42", v)
	}
}

func TestCallFuncG0P3(t *testing.T) {
	var out uintptr
	CallFuncG0P3(C.sum_into, unsafe.Pointer(uintptr(20)), unsafe.Pointer(uintptr(22)), unsafe.Pointer(&out))
	if out != 42 {
		t.Fatalf("out = %d, want 42", out)
	}
}

// The goroutine-stack (non-G0) variants require callees that use no stack
// space; all callees above are trivial single-expression functions.
func TestCallFuncP0(t *testing.T) {
	C.reset_counter()
	CallFuncP0(C.bump_counter)
	if got := uintptr(C.read_counter()); got != 1 {
		t.Fatalf("counter = %d, want 1", got)
	}
}

func TestCallFuncP1(t *testing.T) {
	var v uintptr = 41
	CallFuncP1(C.inc_ptr, unsafe.Pointer(&v))
	if v != 42 {
		t.Fatalf("v = %d, want 42", v)
	}
}

func TestCallFuncP2(t *testing.T) {
	var v uintptr = 40
	CallFuncP2(C.add_to, unsafe.Pointer(uintptr(2)), unsafe.Pointer(&v))
	if v != 42 {
		t.Fatalf("v = %d, want 42", v)
	}
}

func TestCallFuncP3(t *testing.T) {
	var out uintptr
	CallFuncP3(C.sum_into, unsafe.Pointer(uintptr(20)), unsafe.Pointer(uintptr(22)), unsafe.Pointer(&out))
	if out != 42 {
		t.Fatalf("out = %d, want 42", out)
	}
}
