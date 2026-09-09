# Mem Ring

A ring based on shared memory bridging rust and go. It support both tokio and monoio runtime.

With 2 rings, users can simulate calls between rust and go(Both sides can start calls).

The Go package (`mem-ring`) is unix-only: it relies on `x/sys/unix` and socketpair fds, and does not compile on Windows. The unix GOOS set is spelled out in the build constraints (`aix || android || darwin || ...`) instead of the `unix` tag, which Go only recognizes since 1.19 — the package builds with the Go 1.18 minimum toolchain declared in the root `go.mod`.

## How it Works
TODO

## How to Choose Mode for Rust

The `monoio` (default) and `tokio` features are mutually exclusive: enable exactly one of them. Enabling both fails at compile time, because the runtime branches are selected with `all(feature = "tokio", not(feature = "monoio"))` gates and the combination would otherwise silently pick the monoio internals.

### For Tokio Users
```toml
[dependencies]
mem-ring = { version = "0.1", default-features = false, features = ["tokio"] }
```

### For Monoio Users
1. Share a global queue between threads(not enable `tpc`): The aggregation will be better, there will be fewer syscall trigger. But, each consumer must grab the lock, which will introduce competition. Also, since there can only be one consumer per queue, the performance will be limited to a single thread. However, you can dispatch tasks to other workers manually to make it able to to support more throughput(of cause you have to pay for across-thread communication).
2. Use a separate queue for each thread(enable `tpc` makes the performance better for this mode): Each thread has its own queue, which can be consumed or produced independently. But, the aggregation effect will be worse, and the number of syscalls will increase.

I suggest using the second mode if you use monoio, which is the default feature.
```toml
[dependencies]
mem-ring = { version = "0.1" }
```

## Custom Waiter (Go side)

`ReadQueue.RunHandler(handler, w ...TinyWaiter)` consumes the queue in a loop and yields the CPU through a `TinyWaiter` (see `waiter.go`) while there is nothing to read. The default is `GoSchedWaiter`, which is based on `runtime.Gosched`. To customize the wait strategy, pass your own implementation of the `TinyWaiter` interface.

## Stopping background goroutines (Go side)

`ReadQueue.RunHandler` returns a `*Guard`, and `Queue.Write` returns a `WriteQueue` with `Stop`/`Done` methods. `Stop` is idempotent: it signals the background goroutine and closes the notification socket so a goroutine blocked in `Awaiter.Wait` wakes up and exits instead of spinning on a dead fd. `Done` returns a channel that closes once the goroutine has fully exited.

`Notifier.Notify`, `Awaiter.Wait` and `NewAwaiter` report errors to make this possible: a closed or broken fd surfaces as an error rather than a silent busy loop.

## Stopping background tasks (Rust side)

`ReadQueue::run_handler` returns a `Guard`; dropping it stops the read-side handler task. On the write side, `Queue::write` spawns an unstuck handler that flushes pending items; it stops once the **last** `WriteQueue` clone is dropped (all clones share one stop signal), after which pending items are no longer flushed.

One exception: `rust2go-mem-ffi`'s `init_mem_ffi` intentionally keeps both handlers alive forever — it runs once per thread and leaks the read-side guard, and its handler closure holds a `WriteQueue` clone for drop-ack payloads, so the write-side handler never observes the stop signal on that path.

