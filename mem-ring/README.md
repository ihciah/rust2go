# Mem Ring

A ring based on shared memory bridging rust and go. It support both tokio and monoio runtime.

With 2 rings, users can simulate calls between rust and go(Both sides can start calls).

The Go package (`mem-ring`) is unix-only: it relies on `x/sys/unix` and socketpair fds, and does not compile on Windows. The unix GOOS set is spelled out in the build constraints (`aix || android || darwin || ...`) instead of the `unix` tag, which Go only recognizes since 1.19 — the package builds with the Go 1.18 minimum toolchain declared in the root `go.mod`.

## How it Works

Each ring is a SPSC queue living in memory shared by Rust and Go (both languages are linked into one process, so "shared memory" is just the common address space). A ring consists of a fixed-size slot buffer plus four atomics: `head`, `tail`, `working` and `stuck`. The producer writes a slot and bumps `tail`; the consumer reads a slot and bumps `head`, both with acquire/release ordering — there is no lock on the data path.

One side creates a ring with `Queue::new(size)`, which allocates the buffer and the atomics and returns a `QueueMeta` next to the queue. `QueueMeta` is a plain `#[repr(C)]` struct carrying the raw addresses (as `usize`/`uintptr`) and the peer ends of two socketpairs; passing it to the other language lets that side reconstruct a view of the same ring (`Queue::new_from_meta` on Rust, `NewQueue` on Go). The creator owns the memory and frees it on drop; the side built from the meta owns only the fds it received. (Materializing pointers from raw integers trips `go vet`'s unsafeptr analyzer, which is why CI runs vet with `-unsafeptr=false`.)

Polling alone would burn CPU, so each ring also carries two unix socketpairs for notification (the module is called `eventfd`, but it is implemented with `socketpair(AF_UNIX, SOCK_STREAM)`, nonblocking where the OS supports it). A `Notifier` writes a single byte; an `Awaiter` performs one read:

- **working fd** (writer → reader): "there are new items". The writer notifies only on the idle→working transition (the `working` flag), so under load many pushes are aggregated into few syscalls.
- **unstuck fd** (reader → writer): "I drained the ring". When the ring is full, the writer sets `stuck` and parks overflow items in a local pending queue; a background unstuck handler flushes them into the ring each time it is woken.

```
        one ring = slot buffer + head/tail/working/stuck atomics + 2 socketpairs

 writer ── push slot, bump tail ──► [ ring buffer ] ── pop slot, bump head ──► reader
 writer ─────── working fd: 1 byte, "new items" ───────────────────────────► reader
 writer ◄────── unstuck fd: 1 byte, "ring drained" ───────────────────────── reader
```

The read-side handler drains the ring, then yields a few times before sleeping (three `yield_now` rounds on Rust; a `TinyWaiter` on Go, `GoSchedWaiter` by default) to catch pushes racing with the sleep transition, clears `working`, and blocks on the working fd until the peer notifies.

A bidirectional channel is just two independent rings, crossed: each side holds the write half of one ring and the read half of the other, so both Rust and Go can initiate calls. `rust2go-mem-ffi::init_mem_ffi` shows the pattern — `init_rings` creates both rings, hands the two `QueueMeta`s to the peer through its init function, and keeps one `ReadQueue` and one `WriteQueue`. In-flight call state is kept per side in a slab (Go: `MultiSlab`; Rust: `slab::Slab<TaskDesc>`) — an index-keyed local store, not a shared-memory allocator; the indices travel inside `Payload.user_data`/`next_user_data`, and `DROP` payloads tell the peer to release its slot.

On the Rust side the `monoio` (default) and `tokio` features select the same loop structure with different primitives: monoio spawns the handlers on the current thread and uses `local-sync` channels, while tokio uses its own oneshot/spawn with `Send` bounds (the `read_with_tokio_handle`/`write_with_tokio_handle` variants let you pick the runtime). With `monoio + tpc` (the default) the write-side state is `Rc<UnsafeCell>` — single-threaded, lock-free, one queue per thread; without `tpc` it is `Arc<Mutex>` so `WriteQueue` clones can be shared across threads at the cost of lock contention.

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

