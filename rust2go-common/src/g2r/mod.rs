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

    fn try_from(trat: &ItemTrait) -> Result<Self> {
        let trait_name = trat.ident.clone();
        let mut fns = Vec::new();

        for item in trat.items.iter() {
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
