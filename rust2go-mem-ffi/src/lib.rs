// Copyright 2024 ihciah. All Rights Reserved.

// The runtime branches are selected with `all(feature = "tokio", not(feature = "monoio"))`
// gates, so enabling both features would silently select the monoio internals
// while dependents observe tokio as enabled. Reject the combination instead.
#[cfg(all(feature = "monoio", feature = "tokio"))]
compile_error!("features `monoio` and `tokio` are mutually exclusive; enable exactly one");

mod future;
mod utils;

use std::io;

pub use future::*;
pub use mem_ring::{Queue, QueueMeta, ReadQueue, WriteQueue};
pub use slab::Slab;
pub use utils::*;

pub type TaskHandler = fn(usize, TaskDesc) -> bool;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Payload {
    // Request parameters or Response parameters ptr.
    // For multiple parameters, these parameters should be put contiguously in memory.
    pub ptr: usize,
    // For response, user_data should be equal to request's user_data.
    // For drop ack, user_data should be equal to response's next_user_data.
    pub user_data: usize,
    // Use for combined response and drop ack.
    pub next_user_data: usize,
    // Each call with different signature should have a unique call_id.
    pub call_id: u32,
    // last bit: 1=contain request
    // last second bit: 1=contain response
    // last third bit: 1=want peer reply
    // last 4th bit: 1=can drop last payload
    // so:
    // 1. 0b0101=call
    // 2. 0b1110=response to normal call
    // 3. 0b1000=only drop(for response)
    // For a oneway call: send 1, recv 3
    // For a normal call: send 1, recv 2, send 3
    pub flag: u32,
}

impl Payload {
    const CALL: u32 = 0b0101;
    const REPLY: u32 = 0b1110;
    const DROP: u32 = 0b1000;

    #[inline]
    pub const fn new_call(call_id: u32, user_data: usize, ptr: usize) -> Self {
        Self {
            ptr,
            user_data,
            next_user_data: 0,
            call_id,
            flag: Self::CALL,
        }
    }

    #[inline]
    pub fn new_reply(call_id: u32, user_data: usize, next_user_data: usize, ptr: usize) -> Self {
        Self {
            ptr,
            user_data,
            next_user_data,
            call_id,
            flag: Self::REPLY,
        }
    }

    #[inline]
    pub fn new_drop(call_id: u32, user_data: usize) -> Self {
        Self {
            ptr: 0,
            user_data,
            next_user_data: 0,
            call_id,
            flag: Self::DROP,
        }
    }
}

pub struct TaskDesc {
    pub buf: Vec<u8>,
    pub params_ptr: usize,
    pub slot_ptr: usize,
}

/// # Safety
/// peer_init_function_pointer must be a valid function.
// Must be called for each thread.
pub unsafe fn init_mem_ffi<const N: usize>(
    peer_init_function_pointer: *const (),
    size: usize,
    handlers: [TaskHandler; N],
) -> (WriteQueue<Payload>, SharedSlab) {
    let (read_queue, write_queue) =
        init_rings(peer_init_function_pointer, size).expect("unable to init ring");

    #[cfg(all(feature = "tokio", not(feature = "monoio")))]
    let shared_slab = std::sync::Arc::new(std::sync::Mutex::new(Slab::new()));
    #[cfg(not(all(feature = "tokio", not(feature = "monoio"))))]
    let shared_slab = std::rc::Rc::new(std::cell::UnsafeCell::new(Slab::new()));

    let wq = write_queue.clone();
    let sb = shared_slab.clone();
    let guard = read_queue
        .run_handler(move |payload: Payload| {
            let Some(call_handle) = handlers.get(payload.call_id as usize) else {
                panic!("call handler {} not found", payload.call_id);
            };
            let sid = payload.user_data;
            let desc = {
                #[cfg(not(all(feature = "tokio", not(feature = "monoio"))))]
                let locked = unsafe { &mut *sb.get() };
                #[cfg(all(feature = "tokio", not(feature = "monoio")))]
                let mut locked = sb.lock().unwrap();
                locked.remove(sid)
            };

            if call_handle(payload.ptr, desc) {
                let drop_payload = Payload::new_drop(payload.call_id, payload.next_user_data);
                wq.push(drop_payload);
            }
        })
        .expect("unable to run ffi handler");
    Box::leak(Box::new(guard));
    (write_queue, shared_slab)
}

/// # Safety
/// peer_init_function_pointer must be a valid function.
#[cfg(not(all(feature = "tokio", not(feature = "monoio"))))]
pub unsafe fn init_rings<T: 'static>(
    peer_init_function_pointer: *const (),
    size: usize,
) -> Result<(ReadQueue<T>, WriteQueue<T>), io::Error> {
    type RingInitFunc = unsafe extern "C" fn(QueueMeta, QueueMeta);
    let (rqueue, rmeta) = Queue::new(size)?;
    let (wqueue, wmeta) = Queue::new(size)?;

    let init_func: RingInitFunc = std::mem::transmute(peer_init_function_pointer);
    init_func(rmeta, wmeta);

    Ok((rqueue.read(), wqueue.write()?))
}

/// # Safety
/// peer_init_function_pointer must be a valid function.
#[cfg(all(feature = "tokio", not(feature = "monoio")))]
pub unsafe fn init_rings<T: 'static + Send>(
    peer_init_function_pointer: *const (),
    size: usize,
) -> Result<(ReadQueue<T>, WriteQueue<T>), io::Error> {
    type RingInitFunc = unsafe extern "C" fn(QueueMeta, QueueMeta);
    let (rqueue, rmeta) = Queue::new(size)?;
    let (wqueue, wmeta) = Queue::new(size)?;

    let init_func: RingInitFunc = std::mem::transmute(peer_init_function_pointer);
    init_func(rmeta, wmeta);

    Ok((rqueue.read(), wqueue.write()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task_desc(buf: &[u8], params_ptr: usize, slot_ptr: usize) -> TaskDesc {
        TaskDesc {
            buf: buf.to_vec(),
            params_ptr,
            slot_ptr,
        }
    }

    #[test]
    fn payload_new_call() {
        let p = Payload::new_call(7, 100, 0xdead_beef);
        assert_eq!(p.ptr, 0xdead_beef);
        assert_eq!(p.user_data, 100);
        assert_eq!(p.next_user_data, 0);
        assert_eq!(p.call_id, 7);
        assert_eq!(p.flag, 0b0101);
    }

    #[test]
    fn payload_new_reply() {
        let p = Payload::new_reply(3, 10, 20, 0xbeef);
        assert_eq!(p.ptr, 0xbeef);
        assert_eq!(p.user_data, 10);
        assert_eq!(p.next_user_data, 20);
        assert_eq!(p.call_id, 3);
        assert_eq!(p.flag, 0b1110);
    }

    #[test]
    fn payload_new_drop() {
        let p = Payload::new_drop(5, 42);
        assert_eq!(p.ptr, 0);
        assert_eq!(p.user_data, 42);
        assert_eq!(p.next_user_data, 0);
        assert_eq!(p.call_id, 5);
        assert_eq!(p.flag, 0b1000);
    }

    #[test]
    fn payload_flag_protocol() {
        let call = Payload::new_call(0, 0, 0).flag;
        let reply = Payload::new_reply(0, 0, 0, 0).flag;
        let drop = Payload::new_drop(0, 0).flag;

        // These constants are part of the wire protocol with the Go side
        // (see go_shm_ring_init codegen: CALL=0b0101, REPLY=0b1110, DROP=0b1000).
        assert_eq!(call, 0b0101);
        assert_eq!(reply, 0b1110);
        assert_eq!(drop, 0b1000);

        // All flags must be distinct.
        let flags = [call, reply, drop];
        for (i, a) in flags.iter().enumerate() {
            for b in &flags[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn payload_layout_and_copy() {
        // repr(C): three usize fields followed by two u32 fields, no padding.
        assert_eq!(
            std::mem::size_of::<Payload>(),
            3 * std::mem::size_of::<usize>() + 2 * std::mem::size_of::<u32>()
        );
        let p = Payload::new_call(1, 2, 3);
        let copied = p; // Payload is Copy
        assert_eq!(copied.ptr, p.ptr);
        assert_eq!(copied.user_data, p.user_data);
        assert_eq!(copied.call_id, p.call_id);
        assert_eq!(copied.flag, p.flag);
    }

    #[test]
    fn task_desc_fields_and_handler_signature() {
        fn handler(ptr: usize, desc: TaskDesc) -> bool {
            assert_eq!(ptr, 0x10);
            assert_eq!(desc.buf, b"hello");
            true
        }
        let h: TaskHandler = handler;
        assert!(h(0x10, task_desc(b"hello", 1, 2)));

        let desc = task_desc(&[1, 2, 3], 0x10, 0x20);
        assert_eq!(desc.buf, [1, 2, 3]);
        assert_eq!(desc.params_ptr, 0x10);
        assert_eq!(desc.slot_ptr, 0x20);
    }

    #[test]
    fn slab_push_pop_roundtrip() {
        let slab: SharedSlab = new_shared_mut(Slab::new());
        let key1 = push_slab(&slab, task_desc(b"hello", 1, 2));
        let key2 = push_slab(&slab, task_desc(b"", 3, 4));
        assert_ne!(key1, key2);

        let d1 = pop_slab(&slab, key1);
        assert_eq!(d1.buf, b"hello");
        assert_eq!(d1.params_ptr, 1);
        assert_eq!(d1.slot_ptr, 2);

        let d2 = pop_slab(&slab, key2);
        assert!(d2.buf.is_empty());
        assert_eq!(d2.params_ptr, 3);
        assert_eq!(d2.slot_ptr, 4);
    }

    #[test]
    fn slot_inner_basics() {
        let mut slot = SlotInner::<u32>::new();
        assert!(slot.value.is_none());
        assert!(slot.waker.is_none());
        slot.set_result(7);
        assert_eq!(slot.value, Some(7));

        let slot = SlotInner::<u32>::default();
        assert!(slot.value.is_none());
        assert!(slot.waker.is_none());
    }

    #[test]
    fn new_shared_deref() {
        let shared = new_shared(5u32);
        assert_eq!(*shared, 5);
        let cloned = shared.clone();
        assert_eq!(*cloned, 5);
    }

    #[test]
    fn local_fut_resolves_after_set_result() {
        use std::future::Future;
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

        fn noop_waker() -> Waker {
            unsafe fn clone(_: *const ()) -> RawWaker {
                RawWaker::new(std::ptr::null(), &VTABLE)
            }
            unsafe fn wake(_: *const ()) {}
            unsafe fn wake_by_ref(_: *const ()) {}
            unsafe fn drop(_: *const ()) {}
            const VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
            unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
        }

        let slot = new_shared_mut(SlotInner::<u32>::new());
        let fut = LocalFut { slot: slot.clone() };
        let mut fut = std::pin::pin!(fut);

        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        // No value yet: pending, and the waker is registered.
        assert!(matches!(fut.as_mut().poll(&mut cx), Poll::Pending));
        set_result_for_shared_mut_slot(&slot, 42);
        assert!(matches!(fut.as_mut().poll(&mut cx), Poll::Ready(42)));
    }
}
