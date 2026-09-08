// Copyright 2024 ihciah. All Rights Reserved.

use std::any::Any;

pub use rust2go_convert::{
    max_mem_type, CopyStruct, DataView, FromRef, ListRef, MemType, StringRef, ToRef, Writer,
};

mod slot;
pub use slot::{new_atomic_slot, SlotReader, SlotWriter};

mod future;
pub use future::{ResponseFuture, ResponseFutureWithoutReq};

pub use rust2go_macro::{g2r, r2g, r2g_struct_tag, R2G};

pub const DEFAULT_BINDING_FILE: &str = "_go_bindings.rs";
#[macro_export]
macro_rules! r2g_include_binding {
    () => {
        include!(concat!(env!("OUT_DIR"), "/_go_bindings.rs"));
    };
    ($file:literal) => {
        include!(concat!(env!("OUT_DIR"), "/", $file));
    };
}

#[cfg(feature = "build")]
mod builder;
#[cfg(feature = "build")]
pub use builder::{Builder, CopyLib, CustomArgGoCompiler, DefaultGoCompiler, GoCompiler, LinkType};
#[cfg(feature = "build")]
pub use rust2go_gen::GenArgs as RegenArgs;

#[no_mangle]
unsafe extern "C" fn c_rust2go_internal_drop(ptr: *mut ()) {
    drop(Box::from_raw(ptr as *mut dyn Any));
}

#[cfg(test)]
mod tests {
    #[test]
    fn internal_drop_frees_boxed_any() {
        // The generated Go bindings call this through the FFI to release a
        // boxed Rust object; exercise it directly with a boxed value.
        let boxed: Box<dyn std::any::Any> = Box::new(42u32);
        let raw = Box::into_raw(boxed);
        unsafe { super::c_rust2go_internal_drop(raw as *mut ()) };
    }
}
