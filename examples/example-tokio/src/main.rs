mod user;
use std::{
    future::{Future, poll_fn},
    time::Instant,
};

use user::{DemoCall, DemoCallImpl, DemoComplicatedRequest, DemoUser};

#[tokio::main]
async fn main() {
    let user = DemoUser {
        name: "chihai".to_string(),
        age: 28,
    };
    println!("========== Start oneway demo ==========");
    DemoCallImpl::demo_oneway(&user);
    println!("[Rust-oneway] done");

    let req = DemoComplicatedRequest {
        users: vec![user.clone(), user],
        balabala: vec![1],
    };
    println!(
        "[Rust-sync] User pass: {}",
        DemoCallImpl::demo_check(&req).pass
    );

    // Simulate calling a async go function twice.
    // In async way, current thread will not be blocked.
    // So the total time cost will be 3 secs too.
    println!("========== Start async demo ==========");
    let call = Instant::now();
    tokio::join!(async_call(), async_call());
    println!(
        "[Rust-async] Total time cost: {}sec",
        call.elapsed().as_secs()
    );

    println!("========== Start async drop_safe demo ==========");
    drop_safe().await;
}

// Call an async golang function.
// In the golang side, it will sleep 1 seconds to simulate a network io.
async fn async_call() {
    let req = DemoComplicatedRequest {
        users: vec![DemoUser {
            name: "chihai".to_string(),
            age: 28,
        }],
        balabala: vec![1],
    };
    let call = Instant::now();

    // Since we pass reference, there's no way to make future drop safe, so
    // this function is unsafe and user must ensure it would not be dropped.
    let pass = unsafe { DemoCallImpl::demo_check_async(&req).await }.pass;
    println!(
        "[Rust-async] User pass: {pass}, time cost: {}sec",
        call.elapsed().as_secs()
    );
}

// A prove for drop_safe.
async fn drop_safe() {
    let req = DemoComplicatedRequest {
        users: vec![DemoUser {
            name: "chihai".to_string(),
            age: 28,
        }],
        balabala: vec![1],
    };
    let mut fut = DemoCallImpl::demo_check_async_safe(req.clone());

    poll_fn(|cx| {
        // The first poll always returns Pending since we just submitted it.
        assert!(matches!(
            unsafe { std::pin::Pin::new_unchecked(&mut fut).poll(cx) },
            std::task::Poll::Pending
        ));
        println!("[Rust-async drop_safe] Task submitted");
        std::task::Poll::Ready(())
    })
    .await;

    // After poll once, we just leave the function. It is safe.
    drop(fut);
    println!("[Rust-async drop_safe] Task dropped, will sleep 2s to prove golang side memory safe");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    println!("[Rust-async drop_safe] It is expected to see golang side memory safe");
}
