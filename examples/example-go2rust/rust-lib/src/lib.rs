use std::sync::atomic::{AtomicU64, Ordering};

use user::{DemoUser, G2RCall, G2RCallImpl, G2RStatefulCall, G2RStatefulCallImpl};

mod user;

impl G2RCall for G2RCallImpl {
    fn demo_log(name: String, age: u8) {
        println!("[Rust Callee] log user {name} and age {age}");
    }

    fn demo_convert_name(user: DemoUser) -> String {
        let new_user_name = user.name.to_ascii_uppercase();
        println!(
            "[Rust Callee] convert user username: {} -> {new_user_name}",
            user.name
        );
        new_user_name
    }
}

// The stateful trait is implemented on our own struct holding the state.
// Since the instance is shared across Go threads, mutable state must use
// interior mutability (atomics, Mutex, ...).
struct StatefulCounter {
    count: AtomicU64,
}

impl G2RStatefulCall for StatefulCounter {
    fn incr(&self, by: u64) -> u64 {
        let now = self.count.fetch_add(by, Ordering::SeqCst) + by;
        println!("[Rust Callee] counter incr by {by} -> {now}");
        now
    }

    fn current(&self) -> u64 {
        let now = self.count.load(Ordering::SeqCst);
        println!("[Rust Callee] counter current: {now}");
        now
    }
}

/// Called from Go (via cgo) at startup to install the stateful
/// implementation before any g2r call happens. Calling a stateful trait
/// method without registration aborts the process.
#[no_mangle]
pub extern "C" fn rust_lib_init() {
    if G2RStatefulCallImpl::register(StatefulCounter {
        count: AtomicU64::new(0),
    })
    .is_err()
    {
        eprintln!("[Rust] G2RStatefulCallImpl is already registered");
    }
}
