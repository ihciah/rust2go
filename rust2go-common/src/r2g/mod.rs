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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Result<R2GTraitRepr> {
        let item: ItemTrait = syn::parse_str(src).expect("trait should parse");
        R2GTraitRepr::try_from(&item)
    }

    fn err_of(src: &str) -> String {
        // R2GTraitRepr does not implement Debug, so unwrap_err is unavailable.
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
    fn rejects_non_future_impl_trait_return() {
        let err = err_of("pub trait T { fn f() -> impl Send; }");
        assert!(err.contains("only future types are supported"), "{err}");
    }

    #[test]
    fn rejects_async_with_impl_future() {
        let err =
            err_of("pub trait T { async fn f() -> impl std::future::Future<Output = u8>; }");
        assert!(err.contains("async cannot be used with impl Future"), "{err}");
    }

    #[test]
    fn rejects_reference_return() {
        let err = err_of("pub trait T { fn f() -> &'static u8; }");
        assert!(
            err.contains("only path type or impl trait returns are supported"),
            "{err}"
        );
    }

    #[test]
    fn rejects_async_without_return() {
        let err = err_of("pub trait T { async fn f(); }");
        assert!(err.contains("async function must have a return value"), "{err}");
    }

    #[test]
    fn rejects_drop_safe_with_reference_param() {
        let err = err_of("pub trait T { #[drop_safe] async fn f(req: &u8) -> u8; }");
        assert!(
            err.contains("drop_safe function cannot have reference parameters"),
            "{err}"
        );
    }

    #[test]
    fn rejects_sync_shm_with_return() {
        let err = err_of("pub trait T { #[mem] fn f() -> u8; }");
        assert!(
            err.contains("function based on shm must be async or without return value"),
            "{err}"
        );
    }

    #[test]
    fn parses_all_fn_attributes() {
        let repr = parse(
            "pub trait T {
                #[mem] async fn mem_async(x: u8) -> u8;
                #[shm] fn shm_oneway(x: u8);
                #[cgo] fn cgo_alias(x: u8);
                #[cgo_callback] fn cgo_cb(x: u8);
                #[go_pass_struct] fn pass_struct(x: u8);
                #[drop_safe] async fn ds(x: u8) -> u8;
                #[drop_safe_ret] async fn dsr(x: u8) -> u8;
                #[send] async fn send_ret(x: u8) -> u8;
                async fn unsafe_async(x: u8) -> u8;
                async fn ref_param(x: &u8) -> u8;
            }",
        )
        .expect("trait should convert");
        let fns = repr.fns();
        let by = |name: &str| fns.iter().find(|f| f.name() == name).unwrap();

        // Mem calls get sequential ids; async mem without drop_safe is
        // unsafe like any other async fn; sync no-ret shm is unsafe too.
        assert_eq!(by("mem_async").mem_call_id(), Some(0));
        assert!(!by("mem_async").is_safe());
        assert_eq!(by("shm_oneway").mem_call_id(), Some(1));
        assert!(!by("shm_oneway").is_safe());
        // Non-mem calls have no call id.
        assert_eq!(by("cgo_cb").mem_call_id(), None);

        // Both cgo attribute spellings mark the callback.
        assert!(by("cgo_alias").cgo_callback());
        assert!(by("cgo_cb").cgo_callback());
        assert!(!by("mem_async").cgo_callback());

        // go_pass_struct flips go_ptr off; others keep it on.
        assert!(!by("pass_struct").go_ptr);
        assert!(by("cgo_cb").go_ptr);

        // drop_safe variants keep the fn safe; drop_safe_ret marks params
        // return.
        assert!(by("ds").is_safe());
        assert!(!by("ds").drop_safe_ret_params());
        assert!(by("dsr").is_safe());
        assert!(by("dsr").drop_safe_ret_params());

        // #[send] marks ret_send; a plain async fn without drop_safe is
        // unsafe.
        assert!(by("send_ret").ret_send());
        assert!(by("send_ret").ret_static());
        assert!(!by("unsafe_async").is_safe());
        assert!(!by("unsafe_async").ret_send());

        // A reference param flips ret_static off.
        assert!(!by("ref_param").ret_static());
    }
}
