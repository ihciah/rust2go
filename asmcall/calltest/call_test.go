//go:build (linux || darwin || windows) && (amd64 || arm64)
// +build linux darwin windows
// +build amd64 arm64

// Copyright 2024 ihciah. All Rights Reserved.

package calltest

// NOTE: these tests run on linux, darwin and windows. The darwin/arm64 hang of
// the non-G0 trampoline variants observed in CI (macos-latest, Go 1.27) was
// root-caused to the arm64 ASMCALL macro not saving LR (x30): BLR overwrites
// x30 with the trampoline's own return address, so its RET jumped to itself
// and spun. arm64.s now saves/restores x30 (and aligns SP) around the call,
// and the tests exercise both ISAs on both OSes.

import (
	"testing"
	"unsafe"
)

// The goroutine-stack (non-G0) variants require callees that use no stack
// space; all callees are trivial single-expression C functions.

func TestCallFuncG0P0(t *testing.T) {
	ResetCounter()
	CallG0P0Bump()
	CallG0P0Bump()
	if got := ReadCounter(); got != 2 {
		t.Fatalf("counter = %d, want 2", got)
	}
}

func TestCallFuncG0P1(t *testing.T) {
	var v uintptr = 41
	CallG0P1Inc(unsafe.Pointer(&v))
	if v != 42 {
		t.Fatalf("v = %d, want 42", v)
	}
}

func TestCallFuncG0P2(t *testing.T) {
	var v uintptr = 40
	CallG0P2AddTo(2, unsafe.Pointer(&v))
	if v != 42 {
		t.Fatalf("v = %d, want 42", v)
	}
}

func TestCallFuncG0P3(t *testing.T) {
	var out uintptr
	CallG0P3SumInto(20, 22, unsafe.Pointer(&out))
	if out != 42 {
		t.Fatalf("out = %d, want 42", out)
	}
}

func TestCallFuncP0(t *testing.T) {
	ResetCounter()
	CallP0Bump()
	if got := ReadCounter(); got != 1 {
		t.Fatalf("counter = %d, want 1", got)
	}
}

func TestCallFuncP1(t *testing.T) {
	var v uintptr = 41
	CallP1Inc(unsafe.Pointer(&v))
	if v != 42 {
		t.Fatalf("v = %d, want 42", v)
	}
}

func TestCallFuncP2(t *testing.T) {
	var v uintptr = 40
	CallP2AddTo(2, unsafe.Pointer(&v))
	if v != 42 {
		t.Fatalf("v = %d, want 42", v)
	}
}

func TestCallFuncP3(t *testing.T) {
	var out uintptr
	CallP3SumInto(20, 22, unsafe.Pointer(&out))
	if out != 42 {
		t.Fatalf("out = %d, want 42", out)
	}
}
