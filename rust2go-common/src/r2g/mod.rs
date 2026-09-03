// Copyright 2024 ihciah. All Rights Reserved.

mod emit_go;
mod emit_rust;

use quote::format_ident;
use syn::{Error, FnArg, Ident, ItemTrait, Meta, Pat, Result, ReturnType, TraitItem, Type};

use crate::common::{Param, ParamType};

pub struct R2GTraitRepr {
    name: Ident,
    fns: Vec<R2GFnRepr>,
}

impl TryFrom<&ItemTrait> for R2GTraitRepr {
    type Error = Error;

    fn try_from(item_trait: &ItemTrait) -> Result<Self> {
        let trait_name = item_trait.ident.clone();
        let mut fns = Vec::new();

        let mut mem_cnt = 0;
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
            let mut is_async = fn_item.sig.asyncness.is_some();
            let ret = match &fn_item.sig.output {
                ReturnType::Default => None,
                ReturnType::Type(_, t) => match t.as_ref() {
                    Type::Path(_) => {
                        let param_type = ParamType::try_from(t.as_ref())?;
                        Some(param_type)
                    }
                    // Check if it's a future.
                    Type::ImplTrait(i) => {
                        // Find the Output type of the future.
                        let mut output_ty = None;
                        for bound in i.bounds.iter() {
                            let syn::TypeParamBound::Trait(t) = bound else {
                                continue;
                            };
                            let Some(last_seg) = t.path.segments.last() else {
                                continue;
                            };
                            if last_seg.ident != "Future" {
                                continue;
                            }
                            // extract the Output type of the future.
                            let syn::PathArguments::AngleBracketed(a) = &last_seg.arguments else {
                                continue;
                            };
                            if a.args.len() != 1 {
                                continue;
                            }
                            let Some(syn::GenericArgument::AssocType(assoc)) = a.args.first()
                            else {
                                continue;
                            };
                            if assoc.ident != "Output" {
                                continue;
                            }
                            output_ty = Some(&assoc.ty);
                            break;
                        }
                        let output_ty =
                            output_ty.ok_or_else(|| serr!("only future types are supported"))?;
                        // extract the type of the Output.
                        let ret = Some(ParamType::try_from(output_ty)?);
                        if is_async {
                            sbail!("async cannot be used with impl Future");
                        }
                        is_async = true;
                        ret
                    }
                    _ => sbail!("only path type or impl trait returns are supported"),
                },
            };
            if is_async && ret.is_none() {
                sbail!("async function must have a return value")
            }

            // on async mode, parse attributes to check it's drop safe setting.
            let mut drop_safe_ret_params = false;
            let mut ret_send = false;

            let mut is_safe = true;
            let has_reference = params.iter().any(|param| param.ty.is_reference);

            if is_async {
                let drop_safe = fn_item
                .attrs
                .iter()
                .any(|attr|
                    matches!(&attr.meta, Meta::Path(p) if p.get_ident() == Some(&format_ident!("drop_safe")))
                );
                drop_safe_ret_params = fn_item
                .attrs
                .iter()
                .any(|attr|
                    matches!(&attr.meta, Meta::Path(p) if p.get_ident() == Some(&format_ident!("drop_safe_ret")))
                );
                ret_send = fn_item
                .attrs
                .iter()
                .any(|attr|
                    matches!(&attr.meta, Meta::Path(p) if p.get_ident() == Some(&format_ident!("send")))
                );

                if !drop_safe && !drop_safe_ret_params {
                    is_safe = false;
                }
                if (drop_safe || drop_safe_ret_params) && has_reference {
                    sbail!("drop_safe function cannot have reference parameters")
                }
            }

            let go_ptr = fn_item
                .attrs
                .iter()
                .all(|attr|
                    matches!(&attr.meta, Meta::Path(p) if p.get_ident() != Some(&format_ident!("go_pass_struct")))
                );

            let using_mem = fn_item
                .attrs
                .iter()
                .any(|attr|
                    matches!(&attr.meta, Meta::Path(p) if p.get_ident() == Some(&format_ident!("mem")) || p.get_ident() == Some(&format_ident!("shm")))
                );
            let cgo_cb = fn_item
                .attrs
                .iter()
                .any(|attr|
                    matches!(&attr.meta, Meta::Path(p) if p.get_ident() == Some(&format_ident!("cgo_callback")) || p.get_ident() == Some(&format_ident!("cgo")))
                );
            if using_mem && !is_async {
                if ret.is_some() {
                    sbail!("function based on shm must be async or without return value")
                } else {
                    is_safe = false;
                }
            }
            let mem_call_id = if using_mem {
                let id = mem_cnt;
                mem_cnt += 1;
                Some(id)
            } else {
                None
            };

            fns.push(R2GFnRepr {
                name: fn_name,
                is_async,
                params,
                ret,
                is_safe,
                drop_safe_ret_params,
                ret_send,
                ret_static: !has_reference,
                cgo_cb,
                go_ptr,
                mem_call_id,
            });
        }
        Ok(R2GTraitRepr {
            name: trait_name,
            fns,
        })
    }
}

pub struct R2GFnRepr {
    name: Ident,
    is_async: bool,
    params: Vec<Param>,
    ret: Option<ParamType>,
    is_safe: bool,
    drop_safe_ret_params: bool,
    ret_send: bool,
    ret_static: bool,
    go_ptr: bool,
    cgo_cb: bool,
    mem_call_id: Option<usize>,
}

impl R2GTraitRepr {
    pub fn fns(&self) -> &[R2GFnRepr] {
        &self.fns
    }
}

impl R2GFnRepr {
    pub const fn name(&self) -> &Ident {
        &self.name
    }

    pub const fn is_async(&self) -> bool {
        self.is_async
    }

    pub const fn drop_safe_ret_params(&self) -> bool {
        self.drop_safe_ret_params
    }

    pub const fn is_safe(&self) -> bool {
        self.is_safe
    }

    pub fn params(&self) -> &[Param] {
        &self.params
    }

    pub fn ret(&self) -> Option<&ParamType> {
        self.ret.as_ref()
    }

    pub const fn ret_send(&self) -> bool {
        self.ret_send
    }

    pub const fn ret_static(&self) -> bool {
        self.ret_static
    }

    pub const fn mem_call_id(&self) -> Option<usize> {
        self.mem_call_id
    }

    pub const fn cgo_callback(&self) -> bool {
        self.cgo_cb
    }
}

struct BoolMark {
    mark: bool,
    fmt: &'static str,
}
impl BoolMark {
    fn new(mark: bool, fmt: &'static str) -> Self {
        Self { mark, fmt }
    }
}
impl std::fmt::Display for BoolMark {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.mark {
            return write!(f, "{}", self.fmt);
        }
        Ok(())
    }
}
