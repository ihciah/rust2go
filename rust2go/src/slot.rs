use std::{
    mem::MaybeUninit,
    ptr::NonNull,
    sync::atomic::{
        AtomicU8,
        Ordering::{AcqRel, Acquire},
    },
    task::Waker,
};

/// Create a pair of SlotReader and SlotWriter.
/// There's 2 reasons to use it when async rust to go ffi(Go holds writer and rust holds reader):
/// 1. Rust cannot guarantee trying read before go write.
/// 2. Rust can dealloc the memory before go write by simply drop it if using a Box directly.
#[inline]
pub fn new_atomic_slot<T, A>() -> (SlotReader<T, A>, SlotWriter<T, A>) {
    let inner = SlotInner::new();
    let ptr = unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(inner))) };
    (SlotReader(ptr), SlotWriter(ptr))
}

struct SlotInner<T, A = ()> {
    state: State,
    data: MaybeUninit<T>,
    attachment: Option<A>,
    waker: Option<Waker>,
}

impl<T, A> Drop for SlotInner<T, A> {
    fn drop(&mut self) {
        if self.state.load() & 0b100 != 0 {
            unsafe { self.data.assume_init_drop() };
        }
    }
}

// 0b00x: x=1 means writer is dropped, x=0 means writer is alive.
// 0b0x0: x=1 means reader is dropped, x=0 means reader is alive.
// 0bx00: x=1 means data is written, x=0 means data is not written.
#[repr(transparent)]
struct State(AtomicU8);

impl State {
    // Load with Acquire.
    #[inline]
    fn load(&self) -> u8 {
        self.0.load(Acquire)
    }

    /// Do CAS and return action result.
    fn fetch_update_action<F, O>(&self, mut f: F) -> O
    where
        F: FnMut(u8) -> (O, Option<u8>),
    {
        let mut curr = self.0.load(Acquire);
        loop {
            let (output, next) = f(curr);
            let next = match next {
                Some(next) => next,
                None => return output,
            };

            match self.0.compare_exchange(curr, next, AcqRel, Acquire) {
                Ok(_) => return output,
                Err(actual) => curr = actual,
            }
        }
    }
}

impl<T, A> SlotInner<T, A> {
    #[inline]
    const fn new() -> Self {
        Self {
            state: State(AtomicU8::new(0)),
            data: MaybeUninit::uninit(),
            attachment: None,
            waker: None,
        }
    }

    #[inline]
    fn read(&self) -> Option<T> {
        let mut data = MaybeUninit::uninit();
        let copied = self.state.fetch_update_action(|curr| {
            if curr & 0b101 == 0b101 {
                // data has been written and writer has been dropped(data has been fully written)
                unsafe { data = MaybeUninit::new(self.data.as_ptr().read()) };
                // unset the written bit
                (true, Some(curr & 0b011))
            } else {
                (false, None)
            }
        });

        if copied {
            Some(unsafe { data.assume_init() })
        } else {
            None
        }
    }

    #[inline]
    fn write(&mut self, data: T) -> Option<T> {
        let succ = self.state.fetch_update_action(|curr| {
            if curr & 0b100 != 0 {
                // data has been written or another writer has got this bit(but this would not happen in fact)
                (false, None)
            } else {
                // we got this bit
                (true, Some(0b100 | curr))
            }
        });

        if !succ {
            return Some(data);
        }

        unsafe { self.data.as_mut_ptr().write(data) };
        None
    }
}

#[repr(transparent)]
pub struct SlotReader<T, A = ()>(NonNull<SlotInner<T, A>>);
unsafe impl<T: Send, A: Send> Send for SlotReader<T, A> {}
unsafe impl<T: Send, A: Send> Sync for SlotReader<T, A> {}

impl<T, A> SlotReader<T, A> {
    #[inline]
    pub fn read(&self) -> Option<T> {
        unsafe { self.0.as_ref() }.read()
    }

    /// # Safety
    /// Must be read after attachment write.
    #[inline]
    pub unsafe fn read_with_attachment(&mut self) -> Option<(T, Option<A>)> {
        let inner = unsafe { self.0.as_mut() };
        inner.read().map(|res| (res, inner.attachment.take()))
    }
}

impl<T, A> Drop for SlotReader<T, A> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            if self
                .0
                .as_ref()
                .state
                .fetch_update_action(|curr| (curr & 0b001 != 0, Some(0b010 | curr)))
            {
                drop(Box::from_raw(self.0.as_ptr()));
            }
        }
    }
}

#[repr(transparent)]
pub struct SlotWriter<T, A = ()>(NonNull<SlotInner<T, A>>);
unsafe impl<T: Send, A: Send> Send for SlotWriter<T, A> {}
unsafe impl<T: Send, A: Send> Sync for SlotWriter<T, A> {}

impl<T, A> SlotWriter<T, A> {
    #[inline]
    pub fn write(mut self, data: T) {
        if unsafe { self.0.as_mut() }.write(data).is_none() {
            if let Some(waker) = unsafe { self.0.as_mut().waker.take() } {
                waker.wake();
            }
        }
    }

    #[inline]
    pub fn into_ptr(self) -> *const () {
        let ptr = self.0.as_ptr() as *const ();
        std::mem::forget(self);
        ptr
    }

    /// # Safety
    /// Pointer must be a valid *SlotInner<T>.
    #[inline]
    pub unsafe fn from_ptr(ptr: *const ()) -> Self {
        Self(NonNull::new_unchecked(ptr as _))
    }

    #[inline]
    pub(crate) fn attach(&mut self, attachment: A) -> &mut A {
        unsafe { self.0.as_mut() }.attachment.insert(attachment)
    }

    #[inline]
    pub(crate) fn set_waker(&mut self, waker: Waker) {
        unsafe { self.0.as_mut().waker = Some(waker) };
    }
}

impl<T, A> Drop for SlotWriter<T, A> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            if self
                .0
                .as_ref()
                .state
                .fetch_update_action(|curr| (curr & 0b010 != 0, Some(0b001 | curr)))
            {
                drop(Box::from_raw(self.0.as_ptr()));
            }
        }
    }
}
