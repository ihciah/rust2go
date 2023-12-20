// Now the feature I rely on has not been stableized.
#![feature(waker_getters)]

use criterion::{criterion_group, criterion_main, Criterion};
use tokio::runtime::Builder; // TODO: support monoio executor

use example::user::{DemoCall, DemoCallImpl, DemoComplicatedRequest, DemoResponse, DemoUser};

fn r2g_sync() {
    let req = DemoComplicatedRequest {
        users: vec![DemoUser {
            name: "chihai".to_string(),
            age: 28,
        }],
        balabala: vec![1],
    };
    let _resp = DemoCallImpl::demo_check(&req);
}

async fn r2g_async() {
    let req = DemoComplicatedRequest {
        users: vec![DemoUser {
            name: "chihai".to_string(),
            age: 28,
        }],
        balabala: vec![1],
    };
    unsafe {
        let _resp = DemoCallImpl::demo_check_async(&req).await;
    }
}

struct R2RDemoCallImpl;

impl DemoCall for R2RDemoCallImpl {
    fn demo_oneway(_req: &DemoUser) {
        todo!()
    }

    fn demo_check(_req: &DemoComplicatedRequest) -> DemoResponse {
        // user logic
        DemoResponse { pass: true }
    }

    async unsafe fn demo_check_async(_req: &DemoComplicatedRequest) -> DemoResponse {
        // tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        DemoResponse { pass: true }
    }

    async fn demo_check_async_safe(_req: DemoComplicatedRequest) -> DemoResponse {
        // tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        DemoResponse { pass: true }
    }
}
fn r2r_sync() {
    let req = DemoComplicatedRequest {
        users: vec![DemoUser {
            name: "chihai".to_string(),
            age: 28,
        }],
        balabala: vec![1],
    };
    let _resp = R2RDemoCallImpl::demo_check(&req);
}

async fn r2r_async() {
    let req = DemoComplicatedRequest {
        users: vec![DemoUser {
            name: "chihai".to_string(),
            age: 28,
        }],
        balabala: vec![1],
    };
    unsafe {
        let _resp = R2RDemoCallImpl::demo_check_async(&req).await;
    }
}

fn call_sync_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("Call sync bench");

    group.bench_function("r2r_sync", |b| b.iter(r2r_sync));
    group.bench_function("r2g_sync", |b| b.iter(r2g_sync));
    group.finish();
}

fn call_async_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("Call async bench");
    let rt = Builder::new_multi_thread().enable_time().build().unwrap();

    group.bench_function("r2r_async", |b| b.to_async(&rt).iter(r2r_async));
    group.bench_function("r2g_async", |b| b.to_async(&rt).iter(r2g_async));
    group.finish();
}

criterion_group!(r2r_vs_r2g, call_sync_bench, call_async_bench);
criterion_main!(r2r_vs_r2g);
