# Mem Ring

A ring based on shared memory bridging rust and go. It support both tokio and monoio runtime.

With 2 rings, users can simulate calls between rust and go(Both sides can start calls).

## How it Works
TODO

## How to Choose Mode for Rust
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

