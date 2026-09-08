// Copyright 2024 ihciah. All Rights Reserved.

macro_rules! or_empty {
    ($flag: expr, $content: expr) => {
        if $flag {
            $content
        } else {
            ""
        }
    };
}

mod emit_c;
mod emit_go;
mod emit_rust;

use quote::format_ident;
use syn::{Error, FnArg, Ident, ItemTrait, Meta, Pat, Result, ReturnType, TraitItem, Type};

use crate::common::{Param, ParamType};

pub struct G2RTraitRepr {
    name: Ident,
    fns: Vec<G2RFnRepr>,
}

pub struct G2RFnRepr {
    name: Ident,
    params: Vec<Param>,
    ret: Option<ParamType>,
    cgo_call: bool,
}

impl TryFrom<&ItemTrait> for G2RTraitRepr {
    type Error = Error;

    fn try_from(item_trait: &ItemTrait) -> Result<Self> {
        let trait_name = item_trait.ident.clone();
        let mut fns = Vec::new();

        for item in item_trait.items.iter() {
            let TraitItem::Fn(fn_item) = item else {
                sbail!("only fn items are supported");
            };
            let fn_name = fn_item.sig.ident.clone();
            let mut params = Vec::new();
            for param in fn_item.sig.inputs.iter() {
                let FnArg::Typed(param) = param else {
                    sbail!("only typed fn args are supported")
                };
                // param name
                let Pat::Ident(param_name) = param.pat.as_ref() else {
                    sbail!("only ident fn args are supported");
                };
                // param type
                let param_type = ParamType::try_from(param.ty.as_ref())?;
                params.push(Param {
                    name: param_name.ident.clone(),
                    ty: param_type,
                });
            }
            if fn_item.sig.asyncness.is_some() {
                sbail!("async is not supported yet when go call rust, manually spawn by your own!");
            }
            let param_type = match &fn_item.sig.output {
                ReturnType::Default => None,
                ReturnType::Type(_, t) => match t.as_ref() {
                    Type::Path(_) => {
                        let param_type = ParamType::try_from(t.as_ref())?;
                        Some(param_type)
                    }
                    _ => sbail!("only path type returns are supported"),
                },
            };
            let ret = param_type;
            let cgo_call = fn_item
                .attrs
                .iter()
                .any(|attr|
                    matches!(&attr.meta, Meta::Path(p) if p.get_ident() == Some(&format_ident!("cgo_call")) || p.get_ident() == Some(&format_ident!("cgo")))
                );
            fns.push(G2RFnRepr {
                name: fn_name,
                params,
                ret,
                cgo_call,
            });
        }

        Ok(G2RTraitRepr {
            name: trait_name,
            fns,
        })
    }
}

impl G2RTraitRepr {
    pub fn fns(&self) -> &[G2RFnRepr] {
        &self.fns
    }

    pub fn has_ret(&self) -> bool {
        self.fns.iter().any(|f| f.ret.is_some())
    }
}

impl G2RFnRepr {
    fn ffi_param_cnt(&self) -> u8 {
        [self.params.is_empty(), self.ret.is_none()]
            .into_iter()
            .filter(|x| !*x)
            .count() as u8
    }

    pub const fn cgo_call(&self) -> bool {
        self.cgo_call
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Result<G2RTraitRepr> {
        let item: ItemTrait = syn::parse_str(src).expect("trait should parse");
        G2RTraitRepr::try_from(&item)
    }

    fn err_of(src: &str) -> String {
        // G2RTraitRepr does not implement Debug, so unwrap_err is unavailable.
        parse(src).err().expect("should err").to_string()
    }

    #[test]
    fn rejects_non_fn_items() {
        let err = err_of("pub trait T { const X: u8; }");
        assert!(err.contains("only fn items are supported"), "{err}");
    }

    #[test]
    fn rejects_receiver_args() {
        let err = err_of("pub trait T { fn f(&self); }");
        assert!(err.contains("only typed fn args are supported"), "{err}");
    }

    #[test]
    fn rejects_non_ident_patterns() {
        let err = err_of("pub trait T { fn f((a, b): (u8, u8)); }");
        assert!(err.contains("only ident fn args are supported"), "{err}");
    }

    #[test]
    fn rejects_async_fns() {
        let err = err_of("pub trait T { async fn f(); }");
        assert!(err.contains("async is not supported yet"), "{err}");
    }

    #[test]
    fn rejects_non_path_return() {
        let err = err_of("pub trait T { fn f() -> impl Send; }");
        assert!(err.contains("only path type returns are supported"), "{err}");
    }

    #[test]
    fn parses_attrs_and_param_counts() {
        let repr = parse(
            "pub trait T {
                fn no_args_no_ret();
                fn with_ret(x: u8) -> u8;
                #[cgo] fn cgo_alias(x: u8);
                #[cgo_call] fn cgo_call_alias();
            }",
        )
        .expect("trait should convert");

        assert!(repr.has_ret());
        let fns = repr.fns();
        let by = |name: &str| fns.iter().find(|f| f.name == name).unwrap();

        assert_eq!(by("no_args_no_ret").ffi_param_cnt(), 0);
        assert_eq!(by("with_ret").ffi_param_cnt(), 2);
        assert_eq!(by("cgo_alias").ffi_param_cnt(), 1);
        assert_eq!(by("cgo_call_alias").ffi_param_cnt(), 0);

        // Both cgo attribute spellings mark the call.
        assert!(by("cgo_alias").cgo_call());
        assert!(by("cgo_call_alias").cgo_call());
        assert!(!by("with_ret").cgo_call());
    }

    #[test]
    fn no_ret_trait_has_no_ret() {
        let repr = parse("pub trait T { fn f(); }").expect("trait should convert");
        assert!(!repr.has_ret());
    }
}
