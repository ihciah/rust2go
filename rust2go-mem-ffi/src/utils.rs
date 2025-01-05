// Copyright 2024 ihciah. All Rights Reserved.

use slab::Slab;

use crate::TaskDesc;

#[cfg(all(feature = "tokio", not(feature = "monoio")))]
pub type SharedMut<T> = std::sync::Arc<std::sync::Mutex<T>>;
#[cfg(not(all(feature = "tokio", not(feature = "monoio"))))]
pub type SharedMut<T> = std::rc::Rc<std::cell::UnsafeCell<T>>;
#[cfg(all(feature = "tokio", not(feature = "monoio")))]
pub type Shared<T> = std::sync::Arc<T>;
#[cfg(not(all(feature = "tokio", not(feature = "monoio"))))]
pub type Shared<T> = std::rc::Rc<T>;

pub type SharedSlab = SharedMut<Slab<TaskDesc>>;

#[inline]
pub fn pop_slab(slab: &SharedSlab, key: usize) -> TaskDesc {
    #[cfg(all(feature = "tokio", not(feature = "monoio")))]
    {
        slab.lock().unwrap().remove(key)
    }
    #[cfg(not(all(feature = "tokio", not(feature = "monoio"))))]
    {
        unsafe { &mut *slab.get() }.remove(key)
    }
}

#[inline]
pub fn push_slab(slab: &SharedSlab, val: TaskDesc) -> usize {
    #[cfg(all(feature = "tokio", not(feature = "monoio")))]
    {
        slab.lock().unwrap().insert(val)
    }
    #[cfg(not(all(feature = "tokio", not(feature = "monoio"))))]
    {
        unsafe { &mut *slab.get() }.insert(val)
    }
}

#[inline]
pub fn new_shared_mut<T>(item: T) -> SharedMut<T> {
    #[cfg(all(feature = "tokio", not(feature = "monoio")))]
    {
        std::sync::Arc::new(std::sync::Mutex::new(item))
    }
    #[cfg(not(all(feature = "tokio", not(feature = "monoio"))))]
    {
        std::rc::Rc::new(std::cell::UnsafeCell::new(item))
    }
}

#[inline]
pub fn new_shared<T>(item: T) -> Shared<T> {
    #[cfg(all(feature = "tokio", not(feature = "monoio")))]
    {
        std::sync::Arc::new(item)
    }
    #[cfg(not(all(feature = "tokio", not(feature = "monoio"))))]
    {
        std::rc::Rc::new(item)
    }
}

/// # Safety
/// Must be a valid pointer.
#[inline]
pub unsafe fn shared_mut_from_raw<T>(slot_ptr: usize) -> SharedMut<T> {
    #[cfg(all(feature = "tokio", not(feature = "monoio")))]
    {
        std::sync::Arc::from_raw(slot_ptr as *const std::sync::Mutex<T>)
    }
    #[cfg(not(all(feature = "tokio", not(feature = "monoio"))))]
    {
        std::rc::Rc::from_raw(slot_ptr as *const std::cell::UnsafeCell<T>)
    }
}
