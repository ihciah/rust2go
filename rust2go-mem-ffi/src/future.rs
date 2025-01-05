// Copyright 2024 ihciah. All Rights Reserved.

use std::{future::Future, task::Waker};

use crate::SharedMut;

pub struct LocalFut<T> {
    pub slot: SharedMut<SlotInner<T>>,
}

pub struct SlotInner<T> {
    pub value: Option<T>,
    pub waker: Option<Waker>,
}

impl<T> Default for SlotInner<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T> SlotInner<T> {
    #[inline]
    pub const fn new() -> Self {
        SlotInner {
            value: None,
            waker: None,
        }
    }

    #[inline]
    pub fn set_result(&mut self, item: T) {
        self.value = Some(item);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

#[inline]
pub fn set_result_for_shared_mut_slot<T>(shared: &SharedMut<SlotInner<T>>, val: T) {
    #[cfg(all(feature = "tokio", not(feature = "monoio")))]
    {
        shared.lock().unwrap().set_result(val)
    }
    #[cfg(not(all(feature = "tokio", not(feature = "monoio"))))]
    {
        unsafe { &mut *shared.get() }.set_result(val)
    }
}

impl<T> Future for LocalFut<T> {
    type Output = T;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        #[cfg(all(feature = "tokio", not(feature = "monoio")))]
        let mut slot = self.slot.lock().unwrap();
        #[cfg(not(all(feature = "tokio", not(feature = "monoio"))))]
        let slot = unsafe { &mut *self.slot.get() };
        if let Some(val) = slot.value.take() {
            return std::task::Poll::Ready(val);
        }
        match &mut slot.waker {
            Some(w) => w.clone_from(cx.waker()),
            None => slot.waker = Some(cx.waker().clone()),
        }
        std::task::Poll::Pending
    }
}
