#![feature(waker_getters)]

#[macro_use]
mod convert;
pub use convert::{DataView, FromRef, ListRef, MemType, StringRef, ToRef, WakerRef, Writer};

mod slot;
pub use slot::{new_atomic_slot, SlotReader, SlotWriter};

mod future;
pub use future::ResponseFuture;

pub use rust2go_macro::R2G;

#[cfg(feature = "gen")]
pub mod raw_file;

#[cfg(feature = "gen")]
mod build;
#[cfg(feature = "gen")]
pub use build::{Builder, LinkType};

pub(crate) const DEFAULT_BINDING_NAME: &str = "_go_bindings.rs";
