// Copyright 2024 ihciah. All Rights Reserved.

use std::any::Any;

pub use rust2go_convert::{
    max_mem_type, CopyStruct, DataView, FromRef, ListRef, MemType, StringRef, ToRef, Writer,
};

mod slot;
pub use slot::{new_atomic_slot, SlotReader, SlotWriter};

mod future;
pub use future::{ResponseFuture, ResponseFutureWithoutReq};

pub use rust2go_macro::{g2r, r2g, R2G};

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
mod build;
#[cfg(feature = "build")]
pub use build::{Builder, CopyLib, CustomArgGoCompiler, DefaultGoCompiler, GoCompiler, LinkType};
#[cfg(feature = "build")]
pub use rust2go_cli::Args as RegenArgs;

#[no_mangle]
unsafe extern "C" fn c_rust2go_internal_drop(ptr: *mut ()) {
    drop(Box::from_raw(ptr as *mut dyn Any));
}
