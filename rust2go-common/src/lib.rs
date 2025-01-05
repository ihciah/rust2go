// Copyright 2024 ihciah. All Rights Reserved.

#[macro_export]
macro_rules! serr {
    ($msg:expr) => {
        ::syn::Error::new(::proc_macro2::Span::call_site(), $msg)
    };
}

#[macro_export]
macro_rules! sbail {
    ($msg:expr) => {
        return Err(::syn::Error::new(::proc_macro2::Span::call_site(), $msg))
    };
}

pub mod common;
pub mod g2r;
pub mod r2g;
