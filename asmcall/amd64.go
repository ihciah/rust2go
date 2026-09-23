//go:build amd64
// +build amd64

// Copyright 2017 petermattis. Copyright 2024 ihciah. All Rights Reserved.
// Part of design is borrowed from github.com/petermattis/fastcgo/call.go

package asmcall

import (
	_ "runtime"
	"unsafe"
)

// Call C ABI function with 0 argument with G0 stack.
//
//go:noescape
//go:nosplit
func CallFuncG0P0(fn unsafe.Pointer)

// Call C ABI function with 1 argument with G0 stack.
//
//go:noescape
//go:nosplit
func CallFuncG0P1(fn, arg0 unsafe.Pointer)

// Call C ABI function with 2 arguments with G0 stack.
//
//go:noescape
//go:nosplit
func CallFuncG0P2(fn, arg0, arg1 unsafe.Pointer)

// Call C ABI function with 3 arguments with G0 stack.
//
//go:noescape
//go:nosplit
func CallFuncG0P3(fn, arg0, arg1, arg2 unsafe.Pointer)

// Call C ABI function with 0 argument with goroutine stack.
// Note: you MUST make sure the function NOT use stack space, or it may stack
// overflow. The stack pointer is not normalized to 16-byte alignment on this
// path (Go only guarantees 8-byte alignment), so the callee must also tolerate
// that; use the G0 variants when the callee needs full ABI alignment.
//
//go:noescape
//go:nosplit
func CallFuncP0(fn unsafe.Pointer)

// Call C ABI function with 1 argument with goroutine stack.
// Note: you MUST make sure the function NOT use stack space, or it may stack
// overflow. See CallFuncP0 for the stack-alignment contract.
//
//go:noescape
//go:nosplit
func CallFuncP1(fn, arg0 unsafe.Pointer)

// Call C ABI function with 2 arguments with goroutine stack.
// Note: you MUST make sure the function NOT use stack space, or it may stack
// overflow. See CallFuncP0 for the stack-alignment contract.
//
//go:noescape
//go:nosplit
func CallFuncP2(fn, arg0, arg1 unsafe.Pointer)

// Call C ABI function with 3 arguments with goroutine stack.
// Note: you MUST make sure the function NOT use stack space, or it may stack
// overflow. See CallFuncP0 for the stack-alignment contract.
//
//go:noescape
//go:nosplit
func CallFuncP3(fn, arg0, arg1, arg2 unsafe.Pointer)
