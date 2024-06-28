use std::{
    collections::VecDeque,
    future::Future,
    io,
    mem::{self, MaybeUninit},
    os::fd::{IntoRawFd, RawFd},
    sync::atomic::{AtomicU32, AtomicUsize, Ordering},
    task::Waker,
};

#[cfg(not(all(feature = "monoio", feature = "tpc")))]
use parking_lot::Mutex;
#[cfg(not(all(feature = "monoio", feature = "tpc")))]
use std::sync::Arc;

#[cfg(all(feature = "monoio", feature = "tpc"))]
use std::{cell::UnsafeCell, rc::Rc};

#[cfg(feature = "monoio")]
use local_sync::oneshot::{channel, Receiver, Sender};

#[cfg(all(feature = "tokio", not(feature = "monoio")))]
use tokio::sync::oneshot::{channel, Receiver, Sender};

#[cfg(feature = "monoio")]
use monoio::{select, spawn};
#[cfg(all(feature = "tokio", not(feature = "monoio")))]
use tokio::{select, spawn};

use crate::{
    eventfd::{Awaiter, Notifier},
    util::yield_now,
};

pub struct Guard {
    _rx: Receiver<()>,
}

pub struct ReadQueue<T> {
    queue: Queue<T>,
    unstuck_notifier: Notifier,
    #[cfg(all(feature = "tokio", not(feature = "monoio")))]
    tokio_handle: Option<tokio::runtime::Handle>,
}

impl<T> ReadQueue<T> {
    #[inline]
    pub fn meta(&self) -> QueueMeta {
        self.queue.meta()
    }

    pub fn pop(&mut self) -> Option<T> {
        let maybe_item = self.queue.pop();
        if self.queue.stuck() {
            self.queue.mark_unstuck();
            self.unstuck_notifier.notify().ok();
        }
        maybe_item
    }

    #[cfg(feature = "monoio")]
    pub fn run_handler(self, handler: impl FnMut(T) + 'static) -> Result<Guard, io::Error>
    where
        T: 'static,
    {
        let mut working_awaiter = unsafe { Awaiter::from_raw_fd(self.queue.working_fd)? };
        working_awaiter.mark_drop(false);
        let (tx, rx) = channel();
        spawn(self.working_handler(working_awaiter, handler, tx));
        Ok(Guard { _rx: rx })
    }

    #[cfg(all(feature = "tokio", not(feature = "monoio")))]
    pub fn run_handler(self, handler: impl FnMut(T) + Send + 'static) -> Result<Guard, io::Error>
    where
        T: Send + 'static,
    {
        let mut working_awaiter = unsafe { Awaiter::from_raw_fd(self.queue.working_fd)? };
        working_awaiter.mark_drop(false);
        let (tx, rx) = channel();
        if let Some(tokio_handle) = self.tokio_handle.clone() {
            tokio_handle.spawn(self.working_handler(working_awaiter, handler, tx));
        } else {
            spawn(self.working_handler(working_awaiter, handler, tx));
        }
        Ok(Guard { _rx: rx })
    }

    #[cfg(feature = "monoio")]
    async fn working_handler(
        mut self,
        mut working_awaiter: Awaiter,
        mut handler: impl FnMut(T),
        mut tx: Sender<()>,
    ) {
        const YIELD_CNT: u8 = 3;
        let mut exit = std::pin::pin!(tx.closed());
        self.queue.mark_working();

        'p: loop {
            while let Some(item) = self.pop() {
                handler(item);
            }

            for _ in 0..YIELD_CNT {
                yield_now().await;
                if !self.queue.is_empty() {
                    continue 'p;
                }
            }

            if !self.queue.mark_unworking() {
                continue;
            }

            select! {
                _ = working_awaiter.wait() => (),
                _ = &mut exit => {
                    return;
                }
            }
            self.queue.mark_working();
        }
    }

    #[cfg(all(feature = "tokio", not(feature = "monoio")))]
    async fn working_handler(
        mut self,
        mut working_awaiter: Awaiter,
        mut handler: impl FnMut(T) + Send,
        mut tx: Sender<()>,
    ) where
        T: Send,
    {
        const YIELD_CNT: u8 = 3;
        let mut exit = std::pin::pin!(tx.closed());
        self.queue.mark_working();

        'p: loop {
            while let Some(item) = self.pop() {
                handler(item);
            }

            for _ in 0..YIELD_CNT {
                yield_now().await;
                if !self.queue.is_empty() {
                    continue 'p;
                }
            }

            if !self.queue.mark_unworking() {
                continue;
            }

            select! {
                _ = working_awaiter.wait() => (),
                _ = &mut exit => {
                    return;
                }
            }
            self.queue.mark_working();
        }
    }
}

pub struct WriteQueue<T> {
    #[cfg(not(all(feature = "monoio", feature = "tpc")))]
    inner: Arc<Mutex<WriteQueueInner<T>>>,
    #[cfg(all(feature = "monoio", feature = "tpc"))]
    inner: Rc<UnsafeCell<WriteQueueInner<T>>>,
    #[cfg(not(all(feature = "monoio", feature = "tpc")))]
    working_notifier: Arc<Notifier>,
    #[cfg(all(feature = "monoio", feature = "tpc"))]
    working_notifier: Rc<Notifier>,
}

impl<T> Clone for WriteQueue<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            working_notifier: self.working_notifier.clone(),
        }
    }
}

impl<T> WriteQueue<T> {
    // Return if the item is put into queue or pending tasks.
    // Note if the task is put into pending tasks, it will be sent to queue when the queue is not full.
    pub fn push(&self, item: T) -> bool {
        #[cfg(not(all(feature = "monoio", feature = "tpc")))]
        let mut inner = self.inner.lock();
        #[cfg(all(feature = "monoio", feature = "tpc"))]
        let inner = unsafe { &mut *self.inner.get() };
        let item = match inner.queue.push(item) {
            Ok(_) => {
                if !inner.queue.working() {
                    inner.queue.mark_working();
                    #[cfg(not(all(feature = "monoio", feature = "tpc")))]
                    drop(inner);
                    let _ = self.working_notifier.notify();
                }
                return true;
            }
            Err(item) => item,
        };

        // The queue is full now
        inner.queue.mark_stuck();
        let pending = PendingTask {
            data: Some(item),
            waiter: None,
        };
        inner.pending_tasks.push_back(pending);
        false
    }

    // Return if the item is put into queue or pending tasks.
    // Note if the task is put into pending tasks, it will be sent to queue when the queue is not full.
    pub fn push_without_notify(&self, item: T) -> bool {
        #[cfg(not(all(feature = "monoio", feature = "tpc")))]
        let mut inner = self.inner.lock();
        #[cfg(all(feature = "monoio", feature = "tpc"))]
        let inner = unsafe { &mut *self.inner.get() };
        let item = match inner.queue.push(item) {
            Ok(_) => return true,
            Err(item) => item,
        };

        // The queue is full now
        inner.queue.mark_stuck();
        let pending = PendingTask {
            data: Some(item),
            waiter: None,
        };
        inner.pending_tasks.push_back(pending);
        false
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        #[cfg(not(all(feature = "monoio", feature = "tpc")))]
        let inner = self.inner.lock();
        #[cfg(all(feature = "monoio", feature = "tpc"))]
        let inner = unsafe { &*self.inner.get() };
        inner.queue.is_empty()
    }

    // If peer is not working, notify it and mark it working.
    // Return notified
    pub fn notify_manually(&self) -> bool {
        #[cfg(not(all(feature = "monoio", feature = "tpc")))]
        let inner = self.inner.lock();
        #[cfg(all(feature = "monoio", feature = "tpc"))]
        let inner = unsafe { &mut *self.inner.get() };

        if inner.queue.working() {
            return false;
        }

        inner.queue.mark_working();
        #[cfg(not(all(feature = "monoio", feature = "tpc")))]
        drop(inner);
        let _ = self.working_notifier.notify();
        true
    }

    pub fn push_with_awaiter(&self, item: T) -> PushResult {
        #[cfg(not(all(feature = "monoio", feature = "tpc")))]
        let mut inner = self.inner.lock();
        #[cfg(all(feature = "monoio", feature = "tpc"))]
        let inner = unsafe { &mut *self.inner.get() };

        let item = match inner.queue.push(item) {
            Ok(_) => {
                if !inner.queue.working() {
                    inner.queue.mark_working();
                    #[cfg(not(all(feature = "monoio", feature = "tpc")))]
                    drop(inner);
                    let _ = self.working_notifier.notify();
                }
                return PushResult::Ok;
            }
            Err(item) => item,
        };

        // The queue is full now
        inner.queue.mark_stuck();
        #[cfg(not(all(feature = "monoio", feature = "tpc")))]
        let waker_slot = Arc::new(Mutex::new(WakerSlot::None));
        #[cfg(all(feature = "monoio", feature = "tpc"))]
        let waker_slot = Rc::new(UnsafeCell::new(WakerSlot::None));
        let pending = PendingTask {
            data: Some(item),
            waiter: Some(waker_slot.clone()),
        };

        inner.pending_tasks.push_back(pending);
        PushResult::Pending(PushJoinHandle { waker_slot })
    }

    async fn unstuck_handler(self, mut unstuck_awaiter: Awaiter, mut tx: Sender<()>) {
        let mut exit = std::pin::pin!(tx.closed());
        loop {
            {
                #[cfg(not(all(feature = "monoio", feature = "tpc")))]
                let mut inner = self.inner.lock();
                #[cfg(all(feature = "monoio", feature = "tpc"))]
                let inner = unsafe { &mut *self.inner.get() };

                while let Some(mut pending_task) = inner.pending_tasks.pop_front() {
                    let data = pending_task.data.take().unwrap();
                    match inner.queue.push(data) {
                        Ok(_) => {
                            if let Some(waiter) = pending_task.waiter {
                                #[cfg(not(all(feature = "monoio", feature = "tpc")))]
                                waiter.lock().wake();
                                #[cfg(all(feature = "monoio", feature = "tpc"))]
                                unsafe {
                                    (*waiter.get()).wake()
                                };
                            }
                        }
                        Err(data) => {
                            pending_task.data = Some(data);
                            inner.pending_tasks.push_front(pending_task);
                            break;
                        }
                    }
                }
                if !inner.queue.working() {
                    inner.queue.mark_working();
                    let _ = self.working_notifier.notify();
                }
                if !inner.pending_tasks.is_empty() {
                    inner.queue.mark_stuck();
                    if !inner.queue.is_full() {
                        continue;
                    }
                }
            }

            select! {
                _ = unstuck_awaiter.wait() => (),
                _ = &mut exit => {
                    return;
                }
            }
        }
    }
}

pub struct WriteQueueInner<T> {
    queue: Queue<T>,
    pending_tasks: VecDeque<PendingTask<T>>,
    _guard: Receiver<()>,
}

impl<T> WriteQueue<T> {
    #[inline]
    pub fn meta(&self) -> QueueMeta {
        #[cfg(not(all(feature = "monoio", feature = "tpc")))]
        {
            self.inner.lock().queue.meta()
        }
        #[cfg(all(feature = "monoio", feature = "tpc"))]
        {
            unsafe { (*self.inner.get()).queue.meta() }
        }
    }
}

struct PendingTask<T> {
    // always Some, Option is for taking temporary
    data: Option<T>,
    #[cfg(not(all(feature = "monoio", feature = "tpc")))]
    waiter: Option<Arc<Mutex<WakerSlot>>>,
    #[cfg(all(feature = "monoio", feature = "tpc"))]
    waiter: Option<Rc<UnsafeCell<WakerSlot>>>,
}

enum WakerSlot {
    None,
    Some(Waker),
    Finished,
}

impl WakerSlot {
    fn wake(&mut self) {
        if let WakerSlot::Some(w) = mem::replace(self, Self::Finished) {
            w.wake();
        }
    }

    fn set_waker(&mut self, w: &Waker) -> bool {
        match self {
            WakerSlot::None => *self = WakerSlot::Some(w.to_owned()),
            WakerSlot::Some(old_waker) => old_waker.clone_from(w),
            WakerSlot::Finished => return true,
        }
        false
    }
}

pub struct Queue<T> {
    buffer_ptr: *mut MaybeUninit<T>,
    buffer_len: usize,

    head_ptr: *mut AtomicUsize,
    tail_ptr: *mut AtomicUsize,
    working_ptr: *mut AtomicU32,
    stuck_ptr: *mut AtomicU32,

    working_fd: RawFd,
    unstuck_fd: RawFd,

    do_drop: bool,
}

unsafe impl<T: Send> Send for Queue<T> {}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct QueueMeta {
    pub buffer_ptr: usize,
    pub buffer_len: usize,
    pub head_ptr: usize,
    pub tail_ptr: usize,
    pub working_ptr: usize,
    pub stuck_ptr: usize,
    pub working_fd: RawFd,
    pub unstuck_fd: RawFd,
}

unsafe impl<T: Sync> Sync for Queue<T> {}

impl<T> Queue<T> {
    pub fn new(size: usize) -> Result<Self, io::Error> {
        let buffer = unsafe {
            let mut v = Vec::<MaybeUninit<T>>::with_capacity(size);
            v.set_len(size);
            v.into_boxed_slice()
        };
        let buffer_slice = Box::leak(buffer);

        let head_ptr = Box::leak(Box::new(AtomicUsize::new(0)));
        let tail_ptr = Box::leak(Box::new(AtomicUsize::new(0)));
        let working_ptr = Box::leak(Box::new(AtomicU32::new(0)));
        let stuck_ptr = Box::leak(Box::new(AtomicU32::new(0)));

        let working_fd = Notifier::new()?.into_raw_fd();
        let unstuck_fd = Notifier::new()?.into_raw_fd();

        Ok(Self {
            buffer_ptr: buffer_slice.as_mut_ptr(),
            buffer_len: size,
            head_ptr,
            tail_ptr,
            working_ptr,
            stuck_ptr,
            working_fd,
            unstuck_fd,
            do_drop: true,
        })
    }

    /// # Safety
    /// Must make sure the meta is valid until the Queue is dropped
    pub unsafe fn new_from_meta(meta: &QueueMeta) -> Result<Self, io::Error> {
        let buffer_slice =
            std::slice::from_raw_parts_mut(meta.buffer_ptr as *mut MaybeUninit<T>, meta.buffer_len);
        let size = buffer_slice.len();
        let head_ptr = meta.head_ptr as *mut AtomicUsize;
        let tail_ptr = meta.tail_ptr as *mut AtomicUsize;
        let working_ptr = meta.working_ptr as *mut AtomicU32;
        let stuck_ptr = meta.stuck_ptr as *mut AtomicU32;
        let working_fd = meta.working_fd;
        let unstuck_fd = meta.unstuck_fd;
        Ok(Self {
            buffer_ptr: buffer_slice.as_mut_ptr(),
            buffer_len: size,
            head_ptr,
            tail_ptr,
            working_ptr,
            stuck_ptr,
            working_fd,
            unstuck_fd,
            do_drop: false,
        })
    }

    #[inline]
    pub fn is_memory_owner(&self) -> bool {
        self.do_drop
    }

    #[inline]
    pub fn meta(&self) -> QueueMeta {
        QueueMeta {
            buffer_ptr: self.buffer_ptr as _,
            buffer_len: self.buffer_len,
            head_ptr: self.head_ptr as _,
            tail_ptr: self.tail_ptr as _,
            working_ptr: self.working_ptr as _,
            stuck_ptr: self.stuck_ptr as _,
            working_fd: self.working_fd,
            unstuck_fd: self.unstuck_fd,
        }
    }

    pub fn read(self) -> ReadQueue<T> {
        let mut unstuck_notifier = unsafe { Notifier::from_raw_fd(self.unstuck_fd) };
        unstuck_notifier.mark_drop(false);
        ReadQueue {
            queue: self,
            unstuck_notifier,
            #[cfg(all(feature = "tokio", not(feature = "monoio")))]
            tokio_handle: None,
        }
    }

    #[cfg(all(feature = "tokio", not(feature = "monoio")))]
    pub fn read_with_tokio_handle(self, tokio_handle: tokio::runtime::Handle) -> ReadQueue<T> {
        let mut unstuck_notifier = unsafe { Notifier::from_raw_fd(self.unstuck_fd) };
        unstuck_notifier.mark_drop(false);
        ReadQueue {
            queue: self,
            unstuck_notifier,
            tokio_handle: Some(tokio_handle),
        }
    }

    #[cfg(feature = "monoio")]
    pub fn write(self) -> Result<WriteQueue<T>, io::Error>
    where
        T: 'static,
    {
        let mut working_notifier = unsafe { Notifier::from_raw_fd(self.working_fd) };
        let mut unstuck_awaiter = unsafe { Awaiter::from_raw_fd(self.unstuck_fd) }?;

        working_notifier.mark_drop(false);
        unstuck_awaiter.mark_drop(false);

        let (tx, rx) = channel();
        let wq = WriteQueue {
            #[cfg(feature = "tpc")]
            inner: Rc::new(UnsafeCell::new(WriteQueueInner {
                queue: self,
                pending_tasks: VecDeque::new(),
                _guard: rx,
            })),
            #[cfg(not(feature = "tpc"))]
            inner: Arc::new(Mutex::new(WriteQueueInner {
                queue: self,
                pending_tasks: VecDeque::new(),
                _guard: rx,
            })),
            #[cfg(feature = "tpc")]
            working_notifier: Rc::new(working_notifier),
            #[cfg(not(feature = "tpc"))]
            working_notifier: Arc::new(working_notifier),
        };

        spawn(wq.clone().unstuck_handler(unstuck_awaiter, tx));

        Ok(wq)
    }

    #[cfg(all(feature = "tokio", not(feature = "monoio")))]
    pub fn write(self) -> Result<WriteQueue<T>, io::Error>
    where
        T: Send + 'static,
    {
        let mut working_notifier = unsafe { Notifier::from_raw_fd(self.working_fd) };
        let mut unstuck_awaiter = unsafe { Awaiter::from_raw_fd(self.unstuck_fd) }?;

        working_notifier.mark_drop(false);
        unstuck_awaiter.mark_drop(false);

        let (tx, rx) = channel();
        let wq = WriteQueue {
            inner: Arc::new(Mutex::new(WriteQueueInner {
                queue: self,
                pending_tasks: VecDeque::new(),
                _guard: rx,
            })),
            working_notifier: Arc::new(working_notifier),
        };

        spawn(wq.clone().unstuck_handler(unstuck_awaiter, tx));

        Ok(wq)
    }

    #[cfg(all(feature = "tokio", not(feature = "monoio")))]
    pub fn write_with_tokio_handle(
        self,
        tokio_handle: &tokio::runtime::Handle,
    ) -> Result<WriteQueue<T>, io::Error>
    where
        T: Send + 'static,
    {
        let mut working_notifier = unsafe { Notifier::from_raw_fd(self.working_fd) };
        let mut unstuck_awaiter = unsafe { Awaiter::from_raw_fd(self.unstuck_fd) }?;

        working_notifier.mark_drop(false);
        unstuck_awaiter.mark_drop(false);

        let (tx, rx) = channel();
        let wq = WriteQueue {
            inner: Arc::new(Mutex::new(WriteQueueInner {
                queue: self,
                pending_tasks: VecDeque::new(),
                _guard: rx,
            })),
            working_notifier: Arc::new(working_notifier),
        };

        tokio_handle.spawn(wq.clone().unstuck_handler(unstuck_awaiter, tx));

        Ok(wq)
    }
}

impl<T> Drop for Queue<T> {
    fn drop(&mut self) {
        if self.do_drop {
            unsafe {
                let slice = std::slice::from_raw_parts_mut(self.buffer_ptr, self.buffer_len);
                let _ = Box::from_raw(slice as *mut [MaybeUninit<T>]);
                let _ = Box::from_raw(self.head_ptr);
                let _ = Box::from_raw(self.tail_ptr);
                let _ = Box::from_raw(self.working_ptr);
                let _ = Box::from_raw(self.stuck_ptr);
                let _ = Notifier::from_raw_fd(self.unstuck_fd);
                let _ = Notifier::from_raw_fd(self.working_fd);
            }
        }
    }
}

pub enum PushResult {
    Ok,
    Pending(PushJoinHandle),
}

pub struct PushJoinHandle {
    #[cfg(all(feature = "monoio", feature = "tpc"))]
    waker_slot: Rc<UnsafeCell<WakerSlot>>,

    #[cfg(not(all(feature = "monoio", feature = "tpc")))]
    waker_slot: Arc<Mutex<WakerSlot>>,
}

impl Future for PushJoinHandle {
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        #[cfg(all(feature = "monoio", feature = "tpc"))]
        let slot = unsafe { &mut *self.waker_slot.get() };
        #[cfg(not(all(feature = "monoio", feature = "tpc")))]
        let mut slot = self.waker_slot.lock();
        if slot.set_waker(cx.waker()) {
            return std::task::Poll::Ready(());
        }
        std::task::Poll::Pending
    }
}

impl<T> Queue<T> {
    pub fn len(&self) -> usize {
        let shead = unsafe { &*self.head_ptr };
        let stail = unsafe { &*self.tail_ptr };
        stail.load(Ordering::Acquire) - shead.load(Ordering::Acquire)
    }

    pub fn is_empty(&self) -> bool {
        let shead = unsafe { &*self.head_ptr };
        let stail = unsafe { &*self.tail_ptr };
        stail.load(Ordering::Acquire) == shead.load(Ordering::Acquire)
    }

    pub fn is_full(&self) -> bool {
        let shead = unsafe { &*self.head_ptr };
        let stail = unsafe { &*self.tail_ptr };
        stail.load(Ordering::Acquire) - shead.load(Ordering::Acquire) == self.buffer_len
    }

    fn push(&mut self, item: T) -> Result<(), T> {
        let shead = unsafe { &*self.head_ptr };
        let stail = unsafe { &*self.tail_ptr };

        let tail = stail.load(Ordering::Relaxed);
        if tail - shead.load(Ordering::Acquire) == self.buffer_len {
            return Err(item);
        }

        unsafe {
            (*self.buffer_ptr.add(tail % self.buffer_len)).write(item);
        }
        stail.store(tail + 1, Ordering::Release);
        Ok(())
    }

    fn pop(&mut self) -> Option<T> {
        let shead = unsafe { &*self.head_ptr };
        let stail = unsafe { &*self.tail_ptr };

        let head = shead.load(Ordering::Relaxed);
        if head == stail.load(Ordering::Acquire) {
            return None;
        }

        let item = unsafe { (*self.buffer_ptr.add(head % self.buffer_len)).assume_init_read() };
        shead.store(head + 1, Ordering::Release);
        Some(item)
    }

    #[inline]
    fn mark_unworking(&self) -> bool {
        unsafe { &*self.working_ptr }.store(0, Ordering::Release);
        if self.is_empty() {
            return true;
        }
        self.mark_working();
        false
    }

    #[inline]
    fn mark_working(&self) {
        unsafe { &*self.working_ptr }.store(1, Ordering::Release);
    }

    #[inline]
    fn working(&self) -> bool {
        unsafe { &*self.working_ptr }.load(Ordering::Acquire) == 1
    }

    #[inline]
    fn mark_unstuck(&self) {
        unsafe { &*self.stuck_ptr }.store(0, Ordering::Release);
    }

    #[inline]
    fn mark_stuck(&self) {
        unsafe { &*self.stuck_ptr }.store(1, Ordering::Release);
    }

    #[inline]
    fn stuck(&self) -> bool {
        unsafe { &*self.stuck_ptr }.load(Ordering::Acquire) == 1
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[cfg(feature = "monoio")]
    use monoio::time::sleep;
    #[cfg(all(feature = "tokio", not(feature = "monoio")))]
    use tokio::time::sleep;

    macro_rules! test {
        ($($i: item)*) => {$(
            #[cfg(feature = "monoio")]
            #[monoio::test(timer_enabled = true)]
            $i

            #[cfg(all(feature = "tokio", not(feature = "monoio")))]
            #[tokio::test]
            $i
        )*};
    }

    test! {
        async fn demo_wake() {
            let (mut tx, mut rx) = channel::<()>();

            let q_read = Queue::<u8>::new(1024).unwrap();
            let meta = q_read.meta();
            let q_write = unsafe { Queue::<u8>::new_from_meta(&meta) }.unwrap();
            let q_read = q_read.read();
            let q_write = q_write.write().unwrap();

            let _guard = q_read
                .run_handler(move |item| {
                    if item == 2 {
                        rx.close();
                    }
                })
                .unwrap();

            q_write.push(1);
            sleep(Duration::from_secs(1)).await;
            q_write.push(2);
            tx.closed().await;
        }

        async fn demo_stuck() {
            let (mut tx, mut rx) = channel::<()>();

            let q_read = Queue::<u8>::new(1).unwrap();
            let meta = q_read.meta();
            let q_write = unsafe { Queue::<u8>::new_from_meta(&meta) }.unwrap();
            let q_read = q_read.read();
            let q_write = q_write.write().unwrap();

            let _guard = q_read
                .run_handler(move |item| {
                    if item == 4 {
                        rx.close();
                    }
                })
                .unwrap();
            println!("pushed {}", q_write.push(1));
            println!("pushed {}", q_write.push(2));
            println!("pushed {}", q_write.push(3));
            println!("pushed {}", q_write.push(4));
            sleep(Duration::from_secs(1)).await;

            tx.closed().await;
        }
    }
}
