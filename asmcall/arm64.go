//go:build arm64
// +build arm64

// Copyright 2024 ihciah. All Rights Reserved.

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
// Note: you MUST make sure the function NOT use stack space, or it may stack overflow.
//
//go:noescape
//go:nosplit
func CallFuncP0(fn unsafe.Pointer)

// Call C ABI function with 1 argument with goroutine stack.
// Note: you MUST make sure the function NOT use stack space, or it may stack overflow.
//
//go:noescape
//go:nosplit
func CallFuncP1(fn, arg0 unsafe.Pointer)

// Call C ABI function with 2 arguments with goroutine stack.
// Note: you MUST make sure the function NOT use stack space, or it may stack overflow.
//
//go:noescape
//go:nosplit
func CallFuncP2(fn, arg0, arg1 unsafe.Pointer)

// Call C ABI function with 3 arguments with goroutine stack.
// Note: you MUST make sure the function NOT use stack space, or it may stack overflow.
//
//go:noescape
//go:nosplit
func CallFuncP3(fn, arg0, arg1, arg2 unsafe.Pointer)
