use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use crate::SlotReader;

pub enum ResponseFuture<Req, Resp, Exec> {
    // go ffi function, request, callback function ptr
    Init(Exec, Req, *const ()),
    // slot
    Executed(SlotReader<Resp>),
    Fused,
}

impl<Req, Resp, Exec> Future for ResponseFuture<Req, Resp, Exec>
where
    // (Waker, Req, *SlotWriter<Resp>, Callback)
    Exec: FnOnce(Waker, Req, *const (), *const ()) + Unpin,
    Req: Unpin,
{
    type Output = Resp;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this {
            Self::Executed(reader) => {
                if let Some(ret) = unsafe { reader.read() } {
                    *this = Self::Fused;
                    return Poll::Ready(ret);
                }
            }
            Self::Init(..) => {
                // replace to take ownership
                let (reader, writer) = crate::slot::new_atomic_slot::<Resp>();
                let w_ptr = writer.into_ptr();

                let (exec, req, cb) = match std::mem::replace(this, Self::Executed(reader)) {
                    Self::Init(exec, req, cb) => (exec, req, cb),
                    Self::Executed(_) => unsafe { std::hint::unreachable_unchecked() },
                    Self::Fused => unsafe { std::hint::unreachable_unchecked() },
                };

                // convert waker and execute the ffi function
                let waker = cx.waker().clone();
                (exec)(waker, req, w_ptr, cb);
            }
            Self::Fused => {
                panic!("Future polled after ready");
            }
        }
        Poll::Pending
    }
}
