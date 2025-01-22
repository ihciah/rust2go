# ASM CALL in Go

## Overview

> For more implementation details please refer to my blog: <https://en.ihcblog.com/rust2go-cgo-asm>

To avoid extensive checks, scheduling, and GC synchronization overhead, replacing CGO with ASM for invoking external functions can achieve higher efficiency.

This package provides four functions (corresponding to 0~3 arguments). Handwritten assembly is used for Go stack switching and ABI conversion on AMD64 and ARM64 platforms. For other platforms, it falls back to a CGO implementation.

[fastcgo](https://github.com/petermattis/fastcgo) and [rustgo](https://words.filippo.io/rustgo/) were the first attempt at a similar optimization. Implemented seven years ago, it is no longer compatible with newer versions of the Go compiler. Drawing inspiration from fastcgo and referring to the Go runtime source code, I made a new implementation. The main differences include how the g pointer is switched, avoidance of asynchronous preemption, and added ARM64 assembly support.

Core Principles:

1. Perform calling convention conversion (Go ABI0 to System V AMD64 ABI / Microsoft x64 Calling Convention).
2. Save the current SP and switch it to the g0 stack (also switch g to g0).
3. Perform the CALL.
4. Switch SP and g back to their original states.

## Performance

I measured the invocation overhead of ASM and CGO methods using an empty function:

```text
go1.18:
goos: linux
goarch: amd64
pkg: github.com/ihciah/rust2go/asmcall/bench
cpu: AMD Ryzen 7 7840HS w/ Radeon 780M Graphics
BenchmarkCgo-16                 40091475                28.48 ns/op
BenchmarkAsm-16                 520479445                2.285 ns/op
BenchmarkAsmLocal-16            670385510                1.774 ns/op
PASS
ok      github.com/ihciah/rust2go/asmcall/bench 3.973s

go1.22:
goos: linux
goarch: amd64
pkg: github.com/ihciah/rust2go/asmcall/bench
cpu: AMD Ryzen 7 7840HS w/ Radeon 780M Graphics
BenchmarkCgo-16                 40595916                29.19 ns/op
BenchmarkAsm-16                 506890142                2.324 ns/op
BenchmarkAsmLocal-16            675166923                1.829 ns/op
PASS
ok      github.com/ihciah/rust2go/asmcall/bench 4.055s
```

As shown, the ASM approach provides a significant performance improvement compared to CGO (`29.19 ns` → `2.324 ns`). When the C function being invoked is relatively simple, this optimization yields a higher proportion of benefits. When we give up stack switching (this case only applies when the caller ensures that the goroutine stack size is large enough to run the C function), the time consumption can be further reduced to `1.829 ns`.

But this does not mean that the implementation of CGO is necessarily bad. I think it is the result of some trade-offs. For simpler C/Rust functions, using ASM is very suitable; but for external functions that take a long time, using this method will cause Go to be unable to asynchronously preempt, and the scheduling delay of goroutines on the thread will increase, affecting the latency of the entire system.

To measure performance in other environments, run the benchmark code included in this package with `go test -bench .` inside the `/asmcall/bench`.

## Standalone Usage

When you only want to use asmcall without rust2go or without rust.

1. Add `"github.com/ihciah/rust2go"` to `go.mod`.
2. Import `"github.com/ihciah/rust2go/asmcall"` in your Go code.
3. To initiate a CGO call from Go, use `CallFuncG0Px` (where `x` corresponds to the number of arguments, `0~3`):
    Example: `asmcall.CallFuncG0P1(fn, arg0)`
4. To switch back to CGO, simply replace `asmcall` with `cgocall`.
5. Additionally, this package provides `CallFuncPx` (where `x` corresponds to the number of arguments, `0~3`) for in-place invocation, suitable for external functions that do not require stack switching (But please note that if you cannot ensure that the stack space required by the external function is zero, using this method may cause memory corruption).

## Using in Rust2Go

1. Rust2Go uses ASMCALL by default for Go → Rust callbacks, requiring no additional setup from you.
2. To switch back to CGO-based calls, add the `#[cgo_callback]` attribute to the trait method. This attribute only affects the generated Go code.

## References

1. <https://github.com/petermattis/fastcgo>
2. <https://words.filippo.io/rustgo/>
3. <https://en.ihcblog.com/rust2go-cgo-asm>
