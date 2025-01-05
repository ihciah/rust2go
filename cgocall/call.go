package cgocall

// Copyright 2024 ihciah. All Rights Reserved.

/*
// hack from: https://stackoverflow.com/a/69904977
__attribute__((weak))
inline void C_CallFuncP0(const void *f) {
	((void (*)())f)();
}
__attribute__((weak))
inline void C_CallFuncP1(const void *f, const void *arg0) {
	((void (*)(const void*))f)(arg0);
}
__attribute__((weak))
inline void C_CallFuncP2(const void *f, const void *arg0, const void *arg1) {
	((void (*)(const void*, const void*))f)(arg0, arg1);
}
__attribute__((weak))
inline void C_CallFuncP3(const void *f, const void *arg0, const void *arg1, const void *arg2) {
	((void (*)(const void*, const void*, const void*))f)(arg0, arg1, arg2);
}
*/
import "C"

import "unsafe"

func CallFuncG0P0(fn unsafe.Pointer) {
	C.C_CallFuncP0(fn)
}

func CallFuncG0P1(fn, arg0 unsafe.Pointer) {
	C.C_CallFuncP1(fn, arg0)
}

func CallFuncG0P2(fn, arg0, arg1 unsafe.Pointer) {
	C.C_CallFuncP2(fn, arg0, arg1)
}

func CallFuncG0P3(fn, arg0, arg1, arg2 unsafe.Pointer) {
	C.C_CallFuncP3(fn, arg0, arg1, arg2)
}

func CallFuncP0(fn unsafe.Pointer) {
	C.C_CallFuncP0(fn)
}

func CallFuncP1(fn, arg0 unsafe.Pointer) {
	C.C_CallFuncP1(fn, arg0)
}

func CallFuncP2(fn, arg0, arg1 unsafe.Pointer) {
	C.C_CallFuncP2(fn, arg0, arg1)
}

func CallFuncP3(fn, arg0, arg1, arg2 unsafe.Pointer) {
	C.C_CallFuncP3(fn, arg0, arg1, arg2)
}
