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
