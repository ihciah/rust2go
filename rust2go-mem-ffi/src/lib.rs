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
    // last 5th bit: want peer quit
    // so:
    // 1. 0b10100=notify peer to quit and wait peer quit reply
    // 2. 0b10000=notify peer to quit
    // For a quit call: send 1, recv 2
    pub flag: u32,
}

impl Payload {
    const CALL: u32 = 0b0101;
    const REPLY: u32 = 0b1110;
    const DROP: u32 = 0b1000;
    const QUIT_INIT: u32 = 0b10100;
    const QUIT_ACK: u32 = 0b10000;

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

    #[inline]
    pub const fn new_quit_init() -> Self {
        Self {
            ptr: 0,
            user_data: 0,
            next_user_data: 0,
            call_id: 0,
            flag: Self::QUIT_INIT,
        }
    }

    #[inline]
    pub const fn new_quit_ack() -> Self {
        Self {
            ptr: 0,
            user_data: 0,
            next_user_data: 0,
            call_id: 0,
            flag: Self::QUIT_ACK,
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
            if payload.flag & Payload::QUIT_ACK == Payload::QUIT_ACK {
                return;
            }
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
    let rqueue = Queue::new(size)?;
    let wqueue = Queue::new(size)?;

    let init_func: RingInitFunc = std::mem::transmute(peer_init_function_pointer);
    init_func(rqueue.meta(), wqueue.meta());

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
    let rqueue = Queue::new(size)?;
    let wqueue = Queue::new(size)?;

    let init_func: RingInitFunc = std::mem::transmute(peer_init_function_pointer);
    init_func(rqueue.meta(), wqueue.meta());

    Ok((rqueue.read(), wqueue.write()?))
}
