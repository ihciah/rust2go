# ASM CALL in Go

## 概述

> 更多实现细节请参考我的 blog: <https://www.ihcblog.com/rust2go-cgo-asm>

为了避免大量检查、调度和GC同步开销，使用 ASM 替代 CGO 执行外部函数可以达到更高的效率。

本 package 提供了 4 个函数（分别对应 0～3 个参数），对于 AMD64 和 ARM64 平台，手写了汇编来执行 Go 栈切换和 ABI 转换；对于其他平台会 fallback 到 CGO 实现。

[fastcgo](https://github.com/petermattis/fastcgo) 和 [rustgo](https://words.filippo.io/rustgo/) 是第一个做类似尝试的，它实现于 7 年前，并不再适用于较新版本的 Golang 编译器。我参考它的实现和 Go runtime 源码给出了新的实现。新的实现主要差异在于 g 的切换方式和避免 async preempt，并新增了 ARM64 的汇编实现。

核心原理：

1. 完成调用约定转换(go abi0 到 System V AMD64 ABI / Microsoft x64 Calling Convention)；
2. 保存当前 SP，并切换 SP 到 g0 栈（也会将 g 切换到 g0）；
3. 完成 CALL；
4. 将 SP 和 g 切换回来

## 性能

我使用一个空函数来测量 ASM 和 CGO 两种方式的调用开销：

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

可以看出相比 CGO 方式，ASM 方式有较大的性能提升（`29.19 ns` -> `2.324 ns`）。当 C 函数较为简单时，该优化的收益占比会相对较高。当我们放弃栈切换时（该情况仅适用于调用者确保 goroutine 栈大小足够运行 C 函数时），耗时可以进一步缩减到 `1.829 ns`。

但这并不代表 CGO 的实现一定很糟糕，我认为这是做了一些权衡的结果。对于较简单的 C/Rust 函数，使用 ASM 很合适；而如果是耗时较长的外部函数，使用这种方式会导致 Go 无法异步抢占，线程上的 goroutines 调度延迟增大，影响整个系统的延迟。

如需测量在其他环境下的性能表现，可以使用 `go test -bench .` 运行本 package 附带的 benchmark 代码。

## 独立使用方式

当你想仅仅使用本包提供的 asmcall，不需要 rust2go 或任何 rust 相关的东西时。

1. 在 `go.mod` 中添加 `"github.com/ihciah/rust2go"`
2. 在 go 代码中添加 `"github.com/ihciah/rust2go/asmcall"`
3. 当需要从 Go 侧发起 CGO 调用时，调用 `CallFuncG0Px`（`x` 根据参数数量可取0～3）:
    `asmcall.CallFuncG0P1(fn, arg0)`
4. 如果需要使用 CGO 方式完成调用，只需要将 `asmcall` 修改为 `cgocall`
5. 另外，本 package 还提供了原地完成调用的 `CallFuncPx`（x 根据参数数量可取0～3）函数，适用于不需要栈切换的外部函数（但请注意，如果你不能保证外部函数所需栈空间为零，使用该方法可能导致内存踩踏）

## 在 Rust2Go 中使用

1. 在 Rust2Go 中默认使用 ASMCALL 完成 Go -> Rust 的回调，你不需要额外做任何事情
2. 如需切换回基于 CGO 的方式，请为 trait method 添加 `#[cgo_callback]` 属性（该属性仅影响生成的 Go 代码）

## 参考

1. <https://github.com/petermattis/fastcgo>
2. <https://words.filippo.io/rustgo/>
3. <https://www.ihcblog.com/rust2go-cgo-asm>
