// Copyright 2024 ihciah. All Rights Reserved.

use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
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

macro_rules! or_empty {
    ($flag: expr, $content: expr) => {
        if $flag {
            $content
        } else {
            ""
        }
    };
}

impl G2RTraitRepr {
    pub fn fns(&self) -> &[G2RFnRepr] {
        &self.fns
    }

    pub fn has_ret(&self) -> bool {
        self.fns.iter().any(|f| f.ret.is_some())
    }

    pub fn to_importc(&self) -> String {
        let prefix = format!("const void c_{}_", self.name);
        let decs = self
            .fns
            .iter()
            .map(|f| match f.ffi_param_cnt() {
                0 => format!("{prefix}{}();\n", f.name),
                1 => format!("{prefix}{}(const void*);\n", f.name),
                _ => format!("{prefix}{}(const void*, const void*);\n", f.name),
            })
            .collect::<Vec<String>>();
        decs.join("")
    }

    pub fn to_go(&self, levels: &HashMap<Ident, u8>) -> String {
        let trait_name = &self.name;
        let struct_name = format!("{trait_name}Impl");
        let mut out = format!("type {struct_name} struct{{}}\n");

        for f in &self.fns {
            let call_type = if f.cgo_call { "cgocall" } else { "asmcall" };
            let ffi_param_cnt = f.ffi_param_cnt();
            let f_name = &f.name;

            let params = f
                .params
                .iter()
                .map(|p| format!("{} *{}", p.name, p.ty.to_go()))
                .collect::<Vec<_>>()
                .join(",");
            let ret = f.ret.as_ref().map_or(String::new(), |ret| ret.to_go());
            let init_slot = or_empty!(f.ret.is_some(), "_internal_slot := [2]unsafe.Pointer{}\n");
            let mut init_params = String::new();
            if !f.params.is_empty() {
                init_params = format!(
                    "_internal_params := [{}]unsafe.Pointer{{}}\n",
                    f.params.len()
                );
            }

            // write function header
            out.push_str(&format!(
                "func ({struct_name}) {f_name}({params}) {ret} {{
                    {init_slot}{init_params}"
            ));

            // convert params
            for (i, p) in f.params.iter().enumerate() {
                // user_ref, user_buffer := cvt_ref(cntDemoUser, refDemoUser)(user)
                // _internal_params[0] = unsafe.Pointer(&user_ref)
                let cnt = p.ty.go_to_c_field_counter(levels).0;
                let ref_ = p.ty.go_to_c_field_converter(levels).0;
                out.push_str(&format!(
                    "{pname}_ref, {pname}_buffer := cvt_ref({cnt}, {ref_})({pname})
                    _internal_params[{i}] = unsafe.Pointer(&{pname}_ref)
                    ",
                    pname = p.name,
                ));
            }

            // call
            let mut call_params = String::new();
            // unsafe.Pointer(&_internal_slot), unsafe.Pointer(&_internal_params)
            if f.ret.is_some() {
                call_params.push_str(", unsafe.Pointer(&_internal_slot)");
            }
            if !f.params.is_empty() {
                call_params.push_str(", unsafe.Pointer(&_internal_params)");
            }
            out.push_str(&format!(
                "{call_type}.CallFuncG0P{ffi_param_cnt}(unsafe.Pointer(C.c_{trait_name}_{f_name}){call_params})\n"
            ));

            // keepalive
            if f.ret.is_some() {
                out.push_str("runtime.KeepAlive(_internal_slot)\n");
            }
            if !f.params.is_empty() {
                out.push_str("runtime.KeepAlive(_internal_params)\n");
            }
            for p in f.params.iter() {
                out.push_str(&format!("runtime.KeepAlive({}_buffer)\n", p.name));
            }

            if let Some(r) = &f.ret {
                // val := ownString(*(*C.StringRef)(_internal_slot[0]))
                // asmcall.CallFuncG0P1(unsafe.Pointer(C.c_rust2go_internal_drop), unsafe.Pointer(_internal_slot[1]))
                // return val
                let cvt = r.c_to_go_field_converter_owned();
                let cty = r.to_c(false);
                out.push_str(&format!("val := {cvt}(*(*C.{cty})(_internal_slot[0]))
                {call_type}.CallFuncG0P1(unsafe.Pointer(C.c_rust2go_internal_drop), unsafe.Pointer(_internal_slot[1]))
                return val
                "));
            }

            out.push_str("}\n");
        }

        out
    }

    // Generate rust impl.
    pub fn generate_rs(&self) -> Result<TokenStream> {
        let trait_name = &self.name;
        let mut fn_entries = Vec::with_capacity(self.fns.len());
        for f in self.fns.iter() {
            let f_name = &f.name;
            let cf_name = format_ident!("c_{}_{}", &self.name, &f.name);
            let slot_expr = f
                .ret
                .as_ref()
                .map(|_| quote! {_internal_slot: *mut [*const (); 2],});
            let mut params_expr = None;
            if !f.params.is_empty() {
                params_expr = Some(quote! {_internal_params: *const *const ()});
            }
            let mut params = Vec::new();
            let mut param_names = Vec::new();
            for (i, p) in f.params.iter().enumerate() {
                let p_name = &p.name;
                let i = i as isize;
                params.push(quote! {
                    let #p_name = _internal_params.offset(#i).read() as *const _;
                    let #p_name = ::rust2go::FromRef::from_ref(unsafe { &*#p_name });
                });
                param_names.push(p.name.clone());
            }

            let bottom = if f.ret.is_some() {
                quote! {
                    let _internal_out = <Self as #trait_name>::#f_name(#(#param_names),*);
                    let (_internal_buf, _internal_out_ref) = ::rust2go::ToRef::calc_ref(&_internal_out);

                    let _internal_boxed_storage = ::std::boxed::Box::new((_internal_out, _internal_out_ref, _internal_buf));
                    let ret_ptr = &_internal_boxed_storage.as_ref().1 as *const _ as *const ();
                    let drop_ptr = ::std::boxed::Box::leak(_internal_boxed_storage as ::std::boxed::Box<dyn ::std::any::Any>) as *mut dyn ::std::any::Any as *mut ();

                    *_internal_slot = [ret_ptr, drop_ptr];
                }
            } else {
                quote! {
                    <Self as #trait_name>::#f_name(#(#param_names),*);
                }
            };

            fn_entries.push(quote! {
                #[no_mangle]
                unsafe extern "C" fn #cf_name(#slot_expr #params_expr) {
                    #(#params)*
                    #bottom
                }
            });
        }

        let impl_struct_name = format_ident!("{}Impl", trait_name);

        Ok(quote! {
            pub struct #impl_struct_name;
            impl #impl_struct_name {
                #(#fn_entries)*
            }
        })
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
