// Copyright 2024 ihciah. All Rights Reserved.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::SlotReader;

impl<Req, Resp, Exec> ResponseFuture<Req, Resp, Exec> {
    pub fn new(exec: Exec, req: Req, callback: *const ()) -> Self {
        Self::Init(exec, req, callback)
    }

    pub fn new_without_req(
        exec: Exec,
        req: Req,
        callback: *const (),
    ) -> ResponseFutureWithoutReq<Req, Resp, Exec> {
        ResponseFutureWithoutReq(Self::Init(exec, req, callback))
    }
}

pub enum ResponseFuture<Req, Resp, Exec> {
    // go ffi function, request, callback function ptr
    Init(Exec, Req, *const ()),
    // slot
    Executed(SlotReader<Resp, (Req, Vec<u8>)>),
    Fused,
}

unsafe impl<Req: Send, Resp: Send, Exec> Send for ResponseFuture<Req, Resp, Exec> {}
unsafe impl<Req: Sync, Resp: Sync, Exec> Sync for ResponseFuture<Req, Resp, Exec> {}

impl<Req, Resp, Exec> Future for ResponseFuture<Req, Resp, Exec>
where
    // Exec: FnOnce(Req, *SlotWriter<Resp>, Callback)
    // Note: Req is usually a tuple.
    Exec: FnOnce(Req::Ref, *const (), *const ()) + Unpin,
    Req: Unpin + crate::ToRef,
{
    type Output = (Resp, Req);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this {
            Self::Executed(reader) => {
                reader.set_waker(cx.waker());
                if let Some((resp, attachment)) = unsafe { reader.read_with_attachment() } {
                    *this = Self::Fused;
                    let (req, _) = attachment.unwrap();
                    return Poll::Ready((resp, req));
                }
            }
            Self::Init(..) => {
                // replace to take ownership
                let (reader, mut writer) = crate::slot::new_atomic_slot::<Resp, (Req, Vec<u8>)>();

                let (exec, req, cb) = match std::mem::replace(this, Self::Executed(reader)) {
                    Self::Init(exec, req, cb) => (exec, req, cb),
                    Self::Executed(_) => unsafe { std::hint::unreachable_unchecked() },
                    Self::Fused => unsafe { std::hint::unreachable_unchecked() },
                };

                let (buf, req_ref) = req.calc_ref();
                writer.attach((req, buf));
                writer.set_waker(cx.waker().clone());

                // execute the ffi function
                let w_ptr = writer.into_ptr();
                (exec)(req_ref, w_ptr, cb);
            }
            Self::Fused => {
                panic!("Future polled after ready");
            }
        }
        Poll::Pending
    }
}

pub struct ResponseFutureWithoutReq<Req, Resp, Exec>(pub ResponseFuture<Req, Resp, Exec>);

impl<Req, Resp, Exec> Future for ResponseFutureWithoutReq<Req, Resp, Exec>
where
    ResponseFuture<Req, Resp, Exec>: Future<Output = (Resp, Req)>,
{
    type Output = Resp;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().0) }
            .poll(cx)
            .map(|r| r.0)
    }
}


#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::sync::Arc;
    use std::task::Wake;

    use super::*;

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    fn noop_waker() -> std::task::Waker {
        std::task::Waker::from(Arc::new(NoopWake))
    }

    #[test]
    fn response_future_ready() {
        // exec mimics the generated code: rebuild the writer from the ptr
        // and write the response back.
        let exec = |req: u32, w_ptr: *const (), _cb: *const ()| {
            let writer = unsafe { crate::SlotWriter::<u32, (u32, Vec<u8>)>::from_ptr(w_ptr) };
            writer.write(req + 1);
        };
        let mut fut = ResponseFuture::<u32, u32, _>::new(exec, 41u32, std::ptr::null());
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        // the first poll executes the ffi call, which writes synchronously here
        assert!(matches!(Pin::new(&mut fut).poll(&mut cx), Poll::Pending));
        match Pin::new(&mut fut).poll(&mut cx) {
            Poll::Ready((resp, req)) => {
                assert_eq!(resp, 42);
                assert_eq!(req, 41);
            }
            Poll::Pending => panic!("expected ready"),
        }
    }

    #[test]
    fn response_future_pending_until_write() {
        // exec only stores the writer ptr; the response is written later.
        let slot_ptr = Cell::new(std::ptr::null::<()>());
        let exec = |_req: u32, w_ptr: *const (), _cb: *const ()| {
            slot_ptr.set(w_ptr);
        };
        let mut fut = ResponseFuture::<u32, u32, _>::new(exec, 1u32, std::ptr::null());
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        assert!(matches!(Pin::new(&mut fut).poll(&mut cx), Poll::Pending));
        assert!(!slot_ptr.get().is_null());
        // still pending since no response has been written
        assert!(matches!(Pin::new(&mut fut).poll(&mut cx), Poll::Pending));

        let writer = unsafe { crate::SlotWriter::<u32, (u32, Vec<u8>)>::from_ptr(slot_ptr.get()) };
        writer.write(100);
        match Pin::new(&mut fut).poll(&mut cx) {
            Poll::Ready((resp, req)) => {
                assert_eq!(resp, 100);
                assert_eq!(req, 1);
            }
            Poll::Pending => panic!("expected ready"),
        }
    }

    #[test]
    fn response_future_without_req_test() {
        let exec = |_req: u32, w_ptr: *const (), _cb: *const ()| {
            let writer = unsafe { crate::SlotWriter::<u32, (u32, Vec<u8>)>::from_ptr(w_ptr) };
            writer.write(9);
        };
        let mut fut = ResponseFuture::<u32, u32, _>::new_without_req(exec, 1u32, std::ptr::null());
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        assert!(matches!(Pin::new(&mut fut).poll(&mut cx), Poll::Pending));
        match Pin::new(&mut fut).poll(&mut cx) {
            Poll::Ready(resp) => assert_eq!(resp, 9),
            Poll::Pending => panic!("expected ready"),
        }
    }

    #[test]
    #[should_panic(expected = "Future polled after ready")]
    fn poll_after_ready_panics() {
        let exec = |_req: u32, w_ptr: *const (), _cb: *const ()| {
            let writer = unsafe { crate::SlotWriter::<u32, (u32, Vec<u8>)>::from_ptr(w_ptr) };
            writer.write(0);
        };
        let mut fut = ResponseFuture::<u32, u32, _>::new(exec, 1u32, std::ptr::null());
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let _ = Pin::new(&mut fut).poll(&mut cx);
        let _ = Pin::new(&mut fut).poll(&mut cx);
        // polling a fused future must panic
        let _ = Pin::new(&mut fut).poll(&mut cx);
    }

    #[test]
    fn drop_future_before_exec() {
        // dropping an un-polled future must not run exec nor crash
        let called = Cell::new(false);
        {
            let exec = |_req: u32, _w_ptr: *const (), _cb: *const ()| {
                called.set(true);
            };
            let _fut = ResponseFuture::<u32, u32, _>::new(exec, 1u32, std::ptr::null());
        }
        assert!(!called.get());
    }

    #[test]
    fn drop_future_after_exec_without_response() {
        // dropping an executed future with no response written:
        // the reader drop must free the slot once the writer is also gone.
        let slot_ptr = Cell::new(std::ptr::null::<()>());
        {
            let exec = |_req: u32, w_ptr: *const (), _cb: *const ()| {
                slot_ptr.set(w_ptr);
            };
            let mut fut = ResponseFuture::<u32, u32, _>::new(exec, 1u32, std::ptr::null());
            let waker = noop_waker();
            let mut cx = Context::from_waker(&waker);
            assert!(matches!(Pin::new(&mut fut).poll(&mut cx), Poll::Pending));
        }

        // rebuild the writer and drop it without writing; must not crash
        let writer = unsafe { crate::SlotWriter::<u32, (u32, Vec<u8>)>::from_ptr(slot_ptr.get()) };
        drop(writer);
    }
}
