//go:build !arm64 && !amd64
// +build !arm64,!amd64

// Copyright 2024 ihciah. All Rights Reserved.

package asmcall

import (
	"unsafe"

	"github.com/ihciah/rust2go/cgocall"
)

// Call C ABI function with 0 argument with CGO.
// This is exactly the same with CallFuncP0 for non-arm64/amd64.
func CallFuncG0P0(fn unsafe.Pointer) {
	cgocall.CallFuncG0P0(fn)
}

// Call C ABI function with 1 argument with CGO.
// This is exactly the same with CallFuncP1 for non-arm64/amd64.
func CallFuncG0P1(fn, arg0 unsafe.Pointer) {
	cgocall.CallFuncG0P1(fn, arg0)
}

// Call C ABI function with 2 arguments with CGO.
// This is exactly the same with CallFuncP2 for non-arm64/amd64.
func CallFuncG0P2(fn, arg0, arg1 unsafe.Pointer) {
	cgocall.CallFuncG0P2(fn, arg0, arg1)
}

// Call C ABI function with 3 arguments with CGO.
// This is exactly the same with CallFuncP3 for non-arm64/amd64.
func CallFuncG0P3(fn, arg0, arg1, arg2 unsafe.Pointer) {
	cgocall.CallFuncG0P3(fn, arg0, arg1, arg2)
}

// Call C ABI function with 0 argument with CGO.
// This is exactly the same with CallFuncG0P0 for non-arm64/amd64.
func CallFuncP0(fn unsafe.Pointer) {
	cgocall.CallFuncP0(fn)
}

// Call C ABI function with 1 argument with CGO.
// This is exactly the same with CallFuncG0P1 for non-arm64/amd64.
func CallFuncP1(fn, arg0 unsafe.Pointer) {
	cgocall.CallFuncP1(fn, arg0)
}

// Call C ABI function with 2 arguments with CGO.
// This is exactly the same with CallFuncG0P2 for non-arm64/amd64.
func CallFuncP2(fn, arg0, arg1 unsafe.Pointer) {
	cgocall.CallFuncP2(fn, arg0, arg1)
}

// Call C ABI function with 3 arguments with CGO.
// This is exactly the same with CallFuncG0P3 for non-arm64/amd64.
func CallFuncP3(fn, arg0, arg1, arg2 unsafe.Pointer) {
	cgocall.CallFuncP3(fn, arg0, arg1, arg2)
}
