use std::{
    mem::MaybeUninit,
    ptr::NonNull,
    sync::atomic::{
        AtomicU8,
        Ordering::{AcqRel, Acquire},
    },
};

/// Create a pair of SlotReader and SlotWriter.
/// There's 2 reasons to use it when async rust to go ffi(Go holds writer and rust holds reader):
/// 1. Rust cannot guarantee trying read before go write.
/// 2. Rust can dealloc the memory before go write by simply drop it if using a Box directly.
pub fn new_atomic_slot<T>() -> (SlotReader<T>, SlotWriter<T>) {
    let inner = SlotInner::new();
    let ptr = unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(inner))) };
    (SlotReader(ptr), SlotWriter(ptr))
}

struct SlotInner<T> {
    state: State,
    data: MaybeUninit<T>,
}

#[repr(transparent)]
struct State(AtomicU8);

impl State {
    // Load with Acquire.
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

impl<T> SlotInner<T> {
    fn new() -> Self {
        Self {
            state: State(AtomicU8::from(0b11)),
            data: MaybeUninit::uninit(),
        }
    }

    /// # Safety
    /// Can only be read once.
    unsafe fn read(&self) -> Option<T> {
        // If the write bit set to zero, we can read it.
        if self.state.load() & 0b01 == 0 {
            Some(unsafe { self.data.as_ptr().read() })
        } else {
            None
        }
    }

    /// # Safety
    /// By design write should only be called once and not simultaneously.
    unsafe fn write(&mut self, data: T) {
        // Write data and set the write bit to zero.
        self.data.as_mut_ptr().write(data);
        self.state
            .fetch_update_action(|curr| ((), Some(curr & 0b10)));
    }
}

#[repr(transparent)]
pub struct SlotReader<T>(NonNull<SlotInner<T>>);
unsafe impl<T: Send> Send for SlotReader<T> {}
unsafe impl<T: Send> Sync for SlotReader<T> {}

impl<T> SlotReader<T> {
    /// Copy and take output
    /// # Safety
    /// Can only be read once.
    #[inline]
    pub unsafe fn read(&self) -> Option<T> {
        self.0.as_ref().read()
    }
}

impl<T> Drop for SlotReader<T> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            if self.0.as_ref().state.fetch_update_action(|curr| {
                if curr & 0b01 != 0 {
                    (false, Some(0b01))
                } else {
                    (true, Some(0b00))
                }
            }) {
                drop(Box::from_raw(self.0.as_ptr()));
            }
        }
    }
}

#[repr(transparent)]
pub struct SlotWriter<T>(NonNull<SlotInner<T>>);
unsafe impl<T: Send> Send for SlotWriter<T> {}
unsafe impl<T: Send> Sync for SlotWriter<T> {}

impl<T> SlotWriter<T> {
    /// # Safety
    /// Can only write once.
    pub unsafe fn write(mut self, data: T) {
        self.0.as_mut().write(data)
    }

    pub fn into_ptr(self) -> *const () {
        let ptr = self.0.as_ptr() as *const ();
        std::mem::forget(self);
        ptr
    }

    /// # Safety
    /// Pointer must be a valid *SlotInner<T>.
    pub unsafe fn from_ptr(ptr: *const ()) -> Self {
        Self(NonNull::new_unchecked(ptr as _))
    }
}

impl<T> Drop for SlotWriter<T> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            if self.0.as_ref().state.fetch_update_action(|curr| {
                if curr & 0b10 != 0 {
                    (false, Some(0b10))
                } else {
                    (true, Some(0b00))
                }
            }) {
                drop(Box::from_raw(self.0.as_ptr()));
            }
        }
    }
}
