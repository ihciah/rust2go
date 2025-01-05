// Copyright 2024 ihciah. All Rights Reserved.

#[cfg(feature = "monoio")]
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

#[cfg(all(feature = "tokio", not(feature = "monoio")))]
pub use tokio::task::yield_now;

#[cfg(feature = "monoio")]
pub async fn yield_now() {
    struct YieldNow {
        yielded: bool,
    }

    impl Future for YieldNow {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if self.yielded {
                return Poll::Ready(());
            }
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }

    YieldNow { yielded: false }.await;
}
