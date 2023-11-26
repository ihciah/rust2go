// Now the feature I rely on has not been stableized.
#![feature(waker_getters)]

mod user;
use monoio::time::Instant;
use user::{DemoCall, DemoCallImpl, DemoRequest};

#[monoio::main]
async fn main() {
    let req = DemoRequest {
        name: "ihciah".to_string(),
        age: 28,
    };
    println!("[sync] User pass: {}", DemoCallImpl::demo_check(req).pass);

    // Simulate calling a async go function twice.
    // In async way, current thread will not be blocked.
    // So the total time cost will be 5 secs too.
    monoio::join!(async_call(), async_call());
}

// Call an async golang function.
// In the golang side, it will sleep 5 seconds to simulate a network io.
async fn async_call() {
    let req = DemoRequest {
        name: "chihai".to_string(),
        age: 28,
    };
    let call = Instant::now();
    let pass = DemoCallImpl::demo_check_async(req).await.pass;
    println!(
        "[async] User pass: {pass}, time cost: {}sec",
        call.elapsed().as_secs()
    );
}
