// Copyright 2024 ihciah. All Rights Reserved.

use std::collections::HashMap;

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    Error, FnArg, Ident, ItemTrait, Meta, Pat, Path, Result, ReturnType, Token, TraitItem, Type,
};

use crate::common::{Param, ParamType};

pub struct R2GTraitRepr {
    name: Ident,
    fns: Vec<R2GFnRepr>,
}

impl TryFrom<&ItemTrait> for R2GTraitRepr {
    type Error = Error;

    fn try_from(trat: &ItemTrait) -> Result<Self> {
        let trait_name = trat.ident.clone();
        let mut fns = Vec::new();

        let mut mem_cnt = 0;
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
                        let t_str = i
                            .bounds
                            .iter()
                            .find_map(|b| match b {
                                syn::TypeParamBound::Trait(t) => {
                                    let last_seg = t.path.segments.last().unwrap();
                                    if last_seg.ident != "Future" {
                                        return None;
                                    }
                                    // extract the Output type of the future.
                                    let arg = match &last_seg.arguments {
                                        syn::PathArguments::AngleBracketed(a)
                                            if a.args.len() == 1 =>
                                        {
                                            a.args.first().unwrap()
                                        }
                                        _ => return None,
                                    };
                                    match arg {
                                        syn::GenericArgument::AssocType(t)
                                            if t.ident == "Output" =>
                                        {
                                            // extract the type of the Output.
                                            let ret = Some(ParamType::try_from(&t.ty).unwrap());
                                            if is_async {
                                                panic!("async cannot be used with impl Future");
                                            }
                                            is_async = true;
                                            ret
                                        }
                                        _ => None,
                                    }
                                }
                                _ => None,
                            })
                            .ok_or_else(|| serr!("only future types are supported"))?;
                        Some(t_str)
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

    // Generate golang exports.
    pub fn generate_go_exports(&self, levels: &HashMap<Ident, u8>) -> String {
        let name = self.name.to_string();
        let mut out: String = self
            .fns
            .iter()
            .map(|f| f.to_go_export(&name, levels))
            .collect();
        let shm_cnt = self.fns.iter().filter(|f| f.mem_call_id.is_some()).count();
        if shm_cnt != 0 {
            let mem_ffi_handles = (0..shm_cnt)
                .map(|id| format!("ringHandle{name}{id}"))
                .collect::<Vec<String>>();
            out.push_str(&format!("//export RingsInit{name}\nfunc RingsInit{name}(crr, crw C.QueueMeta) {{\nringsInit(crr, crw, []func(unsafe.Pointer, *ants.MultiPool, func(interface{{}}, []byte, uint)){{{}}})\n}}\n", mem_ffi_handles.join(",")));
        }
        out
    }

    // Generate golang interface.
    pub fn generate_go_interface(&self) -> String {
        // var DemoCallImpl DemoCall
        // type DemoCall interface {
        //     demo_oneway(req DemoUser)
        //     demo_check(req DemoComplicatedRequest) DemoResponse
        //     demo_check_async(req DemoComplicatedRequest) DemoResponse
        // }
        let name = self.name.to_string();
        let fns = self.fns.iter().map(|f| f.to_go_interface_method());

        let mut out = String::new();
        out.push_str(&format!("var {name}Impl {name}\n"));
        out.push_str(&format!("type {name} interface {{\n"));
        for f in fns {
            out.push_str(&f);
            out.push('\n');
        }
        out.push_str("}\n");
        out
    }

    // Generate rust impl, callbacks and binding mod include.
    pub fn generate_rs(
        &self,
        binding_path: Option<&Path>,
        queue_size: Option<usize>,
    ) -> Result<TokenStream> {
        const DEFAULT_BINDING_MOD: &str = "binding";
        let path_prefix = match binding_path {
            Some(p) => quote! {#p::},
            None => {
                let binding_mod = format_ident!("{DEFAULT_BINDING_MOD}");
                quote! {#binding_mod::}
            }
        };
        let (mut fn_trait_impls, mut fn_callbacks) = (
            Vec::with_capacity(self.fns.len()),
            Vec::with_capacity(self.fns.len()),
        );
        for f in self.fns.iter() {
            fn_trait_impls.push(f.to_rs_impl(&self.name, &path_prefix)?);
            fn_callbacks.push(f.to_rs_callback(&path_prefix)?);
        }

        let trait_name = &self.name;
        let impl_struct_name = format_ident!("{}Impl", trait_name);

        let mem_init_ffi = format_ident!("RingsInit{}", trait_name);
        let mut shm_init = None;
        let mut shm_init_extc = None;
        let mem_cnt = self.fns.iter().filter(|f| f.mem_call_id.is_some()).count();
        let queue_size = queue_size.unwrap_or(4096);
        if mem_cnt != 0 {
            let mem_ffi_handles = (0..mem_cnt).map(|id| format_ident!("mem_ffi_handle{}", id));
            shm_init = Some(quote! {
                ::std::thread_local! {
                    static WS: (::rust2go_mem_ffi::WriteQueue<::rust2go_mem_ffi::Payload>, ::rust2go_mem_ffi::SharedSlab) = {
                        unsafe {::rust2go_mem_ffi::init_mem_ffi(#mem_init_ffi as *const (), #queue_size, [#(#impl_struct_name::#mem_ffi_handles),*])}
                    };
                }
            });
            shm_init_extc = Some(quote! {
                extern "C" {
                    fn #mem_init_ffi(rr: ::rust2go_mem_ffi::QueueMeta, rw: ::rust2go_mem_ffi::QueueMeta);
                }
            })
        }

        Ok(quote! {
            #shm_init_extc
            pub struct #impl_struct_name;
            impl #trait_name for #impl_struct_name {
                #(#fn_trait_impls)*
            }
            impl #impl_struct_name {
                #shm_init
                #(#fn_callbacks)*
            }
        })
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

    fn to_go_export(&self, trait_name: &str, levels: &HashMap<Ident, u8>) -> String {
        let ref_mark = BoolMark::new(self.go_ptr, "&");
        if let Some(mem_call_id) = self.mem_call_id {
            let fn_sig = format!("func ringHandle{trait_name}{mem_call_id}(ptr unsafe.Pointer, pool *ants.MultiPool, post_func func(interface{{}}, []byte, uint)) {{\n");
            let Some(ret) = &self.ret else {
                return format!("{fn_sig}post_func(nil, nil, 0)\n}}\n");
            };

            let mut fn_body = String::new();
            let params_len = self.params().len();
            for (idx, p) in self.params().iter().enumerate() {
                fn_body.push_str(&format!(
                    "{name}:=*(*C.{ref_type})(ptr)\n",
                    name = p.name,
                    ref_type = p.ty.to_c(false)
                ));
                if idx + 1 != params_len {
                    fn_body.push_str(&format!(
                        "ptr=unsafe.Pointer(uintptr(ptr)+unsafe.Sizeof({name}))\n",
                        name = p.name
                    ));
                }
                fn_body.push_str(&format!(
                    "{name}_:={cvt}({name})\n",
                    name = p.name,
                    cvt = p.ty.c_to_go_field_converter(levels).0
                ));
            }
            fn_body.push_str("pool.Submit(func() {\n");
            fn_body.push_str(&format!(
                "resp := {trait_name}Impl.{fn_name}({ref_mark}{params})\n",
                fn_name = self.name,
                params = self
                    .params
                    .iter()
                    .map(|p| format!("{}_", p.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            fn_body.push_str(&format!(
                "resp_ref_size := uint(unsafe.Sizeof(C.{}{{}}))\n",
                ret.to_c(false)
            ));
            let (g2c_cnt, g2c_cvt) = (
                ret.go_to_c_field_counter(levels).0,
                ret.go_to_c_field_converter(levels).0,
            );
            fn_body.push_str(&format!("resp_ref, buffer := cvt_ref_cap({g2c_cnt}, {g2c_cvt}, resp_ref_size)(&resp)\noffset := uint(len(buffer))\nbuffer = append(buffer, unsafe.Slice((*byte)(unsafe.Pointer(&resp_ref)), resp_ref_size)...)\n"));
            fn_body.push_str("post_func(resp, buffer, offset)\n})\n");
            let fn_ending = "}\n";
            return format!("{fn_sig}{fn_body}{fn_ending}");
        }

        let mut out = String::new();
        let fn_name = format!("C{}_{}", trait_name, self.name);
        out.push_str(&format!("//export {fn_name}\nfunc {fn_name}("));
        self.params
            .iter()
            .for_each(|p| out.push_str(&format!("{} C.{}, ", p.name, p.ty.to_c(false))));

        let mut new_names = Vec::new();
        let mut new_cvt = String::new();
        for p in self.params.iter() {
            let new_name = format_ident!("_new_{}", p.name);
            let cvt = p.ty.c_to_go_field_converter(levels).0;
            new_cvt.push_str(&format!("{new_name} := {cvt}({})\n", p.name));
            new_names.push(format!("{ref_mark}{}", new_name));
        }
        match (self.is_async, &self.ret) {
            (true, None) => panic!("async function must have a return value"),
            (false, None) => {
                // //export CDemoCall_demo_oneway
                // func CDemoCall_demo_oneway(req C.DemoUserRef) {
                //     DemoCallImpl.demo_oneway(newDemoUser(req))
                // }
                out.push_str(") {\n");
                out.push_str(&new_cvt);
                out.push_str(&format!(
                    "    {trait_name}Impl.{fn_name}({params})\n",
                    fn_name = self.name,
                    params = new_names.join(", ")
                ));
                out.push_str("}\n");
            }
            (false, Some(ret)) => {
                // //export CDemoCall_demo_check
                // func CDemoCall_demo_check(req C.DemoComplicatedRequestRef, slot *C.void, cb *C.void) {
                //     resp := DemoCallImpl.demo_check(newDemoComplicatedRequest(req))
                //     resp_ref, buffer := cvt_ref(cntDemoResponse, refDemoResponse)(&resp)
                //     C.DemoCall_demo_check_cb(unsafe.Pointer(cb), &resp_ref, unsafe.Pointer(slot))
                //     runtime.KeepAlive(resp_ref)
                //     runtime.KeepAlive(resp)
                //     runtime.KeepAlive(buffer)
                // }
                out.push_str("slot *C.void, cb *C.void) {\n");
                out.push_str(&new_cvt);
                out.push_str(&format!(
                    "resp := {trait_name}Impl.{fn_name}({params})\n",
                    fn_name = self.name,
                    params = new_names.join(", ")
                ));
                let (g2c_cnt, g2c_cvt) = (
                    ret.go_to_c_field_counter(levels).0,
                    ret.go_to_c_field_converter(levels).0,
                );
                out.push_str(&format!(
                    "resp_ref, buffer := cvt_ref({g2c_cnt}, {g2c_cvt})(&resp)\n"
                ));
                if self.cgo_cb {
                    out.push_str("cgocall.CallFuncG0P2(unsafe.Pointer(cb), unsafe.Pointer(&resp_ref), unsafe.Pointer(slot))\n");
                } else {
                    out.push_str("asmcall.CallFuncG0P2(unsafe.Pointer(cb), unsafe.Pointer(&resp_ref), unsafe.Pointer(slot))\n");
                }
                out.push_str("runtime.KeepAlive(resp_ref)\nruntime.KeepAlive(resp)\nruntime.KeepAlive(buffer)\n");
                out.push_str("}\n");
            }
            (true, Some(ret)) => {
                // //export CDemoCall_demo_check_async
                // func CDemoCall_demo_check_async(req C.DemoComplicatedRequestRef, slot *C.void, cb *C.void) {
                //     _new_req := newDemoComplicatedRequest(req)
                //     go func() {
                //         resp := DemoCallImpl.demo_check_async(_new_req)
                //         resp_ref, buffer := cvt_ref(cntDemoResponse, refDemoResponse)(&resp)
                //         C.DemoCall_demo_check_async_cb(unsafe.Pointer(cb), &resp_ref, unsafe.Pointer(slot))
                //         runtime.KeepAlive(resp)
                //         runtime.KeepAlive(resp)
                //         runtime.KeepAlive(buffer)
                //     }()
                // }
                out.push_str("slot *C.void, cb *C.void) {\n");
                out.push_str(&new_cvt);
                out.push_str("    go func() {\n");
                out.push_str(&format!(
                    "resp := {trait_name}Impl.{fn_name}({params})\n",
                    fn_name = self.name,
                    params = new_names.join(", ")
                ));
                let (g2c_cnt, g2c_cvt) = (
                    ret.go_to_c_field_counter(levels).0,
                    ret.go_to_c_field_converter(levels).0,
                );
                out.push_str(&format!(
                    "resp_ref, buffer := cvt_ref({g2c_cnt}, {g2c_cvt})(&resp)\n"
                ));
                if self.cgo_cb {
                    out.push_str("cgocall.CallFuncG0P2(unsafe.Pointer(cb), unsafe.Pointer(&resp_ref), unsafe.Pointer(slot))\n");
                } else {
                    out.push_str("asmcall.CallFuncG0P2(unsafe.Pointer(cb), unsafe.Pointer(&resp_ref), unsafe.Pointer(slot))\n");
                }
                out.push_str("runtime.KeepAlive(resp_ref)\nruntime.KeepAlive(resp)\nruntime.KeepAlive(buffer)\n");
                out.push_str("}()\n}\n");
            }
        }
        out
    }

    fn to_go_interface_method(&self) -> String {
        // demo_oneway(req DemoUser)
        // demo_check(req DemoComplicatedRequest) DemoResponse
        let star_mark = BoolMark::new(self.go_ptr, "*");
        format!(
            "{}({}) {}",
            self.name,
            self.params
                .iter()
                .map(|p| format!("{} {star_mark}{}", p.name, p.ty.to_go()))
                .collect::<Vec<_>>()
                .join(", "),
            self.ret.as_ref().map(|p| p.to_go()).unwrap_or_default()
        )
    }

    fn to_rs_impl(&self, trait_name: &Ident, path_prefix: &TokenStream) -> Result<TokenStream> {
        let mut out = TokenStream::default();

        let func_name = &self.name;
        let callback_name = format_ident!("{func_name}_cb");
        let func_param_names: Vec<_> = self.params.iter().map(|p| &p.name).collect();
        let func_param_types: Vec<_> = self.params.iter().map(|p| &p.ty).collect();
        let unsafe_marker = (!self.is_safe).then(syn::token::Unsafe::default);
        out.extend(quote! {
            #unsafe_marker fn #func_name(#(#func_param_names: #func_param_types),*)
        });

        let ref_marks = self.params.iter().map(|p| {
            if p.ty.is_reference {
                None
            } else {
                Some(Token![&](Span::call_site()))
            }
        });
        let c_func_name = format_ident!("C{trait_name}_{func_name}");
        match (self.is_async, &self.ret) {
            (true, None) => sbail!("async function must have a return value"),
            (false, None) => {
                if let Some(mem_call_id) = self.mem_call_id {
                    // fn demo_oneway(req: &DemoUser) {
                    //     const CALL_ID: u32 = 0;
                    //     let (buf, ptr) = ::rust2go::ToRef::calc_ref(&::rust2go::CopyStruct((&req,)));
                    //     Self::WS.with(|(wq, slab)| {
                    //         let slab = unsafe { &mut *slab.get() };
                    //         let sid = slab.insert(::rust2go_mem_ffi::TaskDesc {
                    //             buf,
                    //             params_ptr: 0,
                    //             slot_ptr: 0,
                    //         });
                    //         wq.push(::rust2go_mem_ffi::Payload::new_call(
                    //             CALL_ID,
                    //             sid,
                    //             ptr as usize,
                    //         ));
                    //     });
                    // }
                    let mem_call_id = mem_call_id as u32;
                    out.extend(quote! {
                        {
                            const CALL_ID: u32 = #mem_call_id;
                            let (buf, ptr) = ::rust2go::ToRef::calc_ref(&::rust2go::CopyStruct((#(&#func_param_names,)*)));
                            Self::WS.with(|(wq, sb)| {
                                let sid = ::rust2go_mem_ffi::push_slab(sb, ::rust2go_mem_ffi::TaskDesc {
                                    buf,
                                    params_ptr: 0,
                                    slot_ptr: 0,
                                });
                                wq.push(::rust2go_mem_ffi::Payload::new_call(
                                    CALL_ID,
                                    sid,
                                    ptr as usize,
                                ));
                            });
                        }
                    });
                } else {
                    // fn demo_check(r: user::DemoRequest) {
                    //     let (_buf, r) = ::rust2go::ToRef::calc_ref(&r);
                    //     unsafe {binding::CDemoCall_demo_check(::std::mem::transmute(r))}
                    // }
                    out.extend(quote! {
                        {
                            #(
                                let (_buf, #func_param_names) = ::rust2go::ToRef::calc_ref(#ref_marks #func_param_names);
                            )*
                            #[allow(clippy::useless_transmute)]
                            unsafe {#path_prefix #c_func_name(#(::std::mem::transmute(#func_param_names)),*)}
                        }
                    });
                }
            }
            (false, Some(ret)) => {
                if self.mem_call_id.is_some() {
                    sbail!("sync function with return value cannot be shm call")
                }
                // fn demo_check(r: user::DemoRequest) -> user::DemoResponse {
                //     let mut slot = None;
                //     let (_buf, r) = ::rust2go::ToRef::calc_ref(&r);
                //     unsafe { binding::CDemoCall_demo_check(
                //         ::std::mem::transmute(r),
                //         &slot as *const _ as *const () as *mut _,
                //         Self::demo_check_cb as *const () as *mut _,
                //     )}
                //     slot.take().unwrap()
                // }

                out.extend(quote!{
                    -> #ret {
                        let mut slot = None;
                        #(
                            let (_buf, #func_param_names) = ::rust2go::ToRef::calc_ref(#ref_marks #func_param_names);
                        )*
                        #[allow(clippy::useless_transmute)]
                        unsafe { #path_prefix #c_func_name(#(::std::mem::transmute(#func_param_names),)* &slot as *const _ as *const () as *mut _, Self::#callback_name as *const () as *mut _) };
                        slot.take().unwrap()
                    }
                });
            }
            (true, Some(ret)) => {
                if let Some(mem_call_id) = self.mem_call_id {
                    // const CALL_ID: u32 = 1;

                    // let (buf, ptr) = ::rust2go::ToRef::calc_ref(&::rust2go::CopyStruct((&req,)));
                    // let slot = ::std::rc::Rc::new(::std::cell::UnsafeCell::new(::rust2go::SlotInner::<
                    //     DemoResponse,
                    // >::new()));
                    // let slot_ptr = ::std::rc::Rc::into_raw(slot.clone()) as usize;

                    // Self::WS.with(|(wq, sb)| {
                    //     let slab = unsafe { &mut *sb.get() };
                    //     let sid = slab.insert(::rust2go_mem_ffi::TaskDesc {
                    //         buf,
                    //         params_ptr: Box::leak(Box::new((req,))) as *const _ as usize,
                    //         slot_ptr,
                    //     });
                    //     let payload = ::rust2go_mem_ffi::Payload::new_call(CALL_ID, sid, ptr as usize);
                    //     println!("[Rust] Send payload: {payload:?}");
                    //     wq.push(payload)
                    // });
                    // ::rust2go::LocalFut { slot }
                    let mem_call_id = mem_call_id as u32;
                    let fut_output = if self.drop_safe_ret_params {
                        quote! { (#ret, (#(#func_param_types,)*)) }
                    } else {
                        quote! { #ret }
                    };
                    out.extend(quote! {
                        -> impl ::std::future::Future<Output = #fut_output> {
                            const CALL_ID: u32 = #mem_call_id;

                            let (buf, ptr) = ::rust2go::ToRef::calc_ref(&::rust2go::CopyStruct((#(&#func_param_names,)*)));
                            let slot = ::rust2go_mem_ffi::new_shared_mut(::rust2go_mem_ffi::SlotInner::<#fut_output>::new());
                            let slot_ptr = ::rust2go_mem_ffi::Shared::into_raw(slot.clone()) as usize;
                            Self::WS.with(|(wq, sb)| {
                                let sid = ::rust2go_mem_ffi::push_slab(sb, ::rust2go_mem_ffi::TaskDesc {
                                    buf,
                                    params_ptr: Box::into_raw(Box::new((#(#func_param_names,)*))) as usize,
                                    slot_ptr,
                                });
                                let payload = ::rust2go_mem_ffi::Payload::new_call(CALL_ID, sid, ptr as usize);
                                wq.push(payload)
                            });
                            ::rust2go_mem_ffi::LocalFut { slot }
                        }
                    });
                } else {
                    // fn demo_check_async(
                    //     req: user::DemoRequest,
                    // ) -> impl std::future::Future<Output = user::DemoResponse> {
                    //     ::rust2go::ResponseFuture::Init(
                    //         |r_ref: <(user::DemoRequest,) as ToRef>::Ref, slot: *const (), cb: *const ()| {
                    //             unsafe {
                    //                 binding::CDemoCall_demo_check_async(
                    //                     ::std::mem::transmute(r_ref.0),
                    //                     slot as *const _ as *mut _,
                    //                     cb as *const _ as *mut _,
                    //                 )
                    //             };
                    //         },
                    //         (req,),
                    //         Self::demo_check_async_cb as *const (),
                    //     )
                    // }
                    let len = self.params.len();
                    let tuple_ids = (0..len).map(syn::Index::from);
                    let new_fn = match self.drop_safe_ret_params {
                        false => quote! {::rust2go::ResponseFuture::new_without_req},
                        true => quote! {::rust2go::ResponseFuture::new},
                    };
                    let ret = match self.drop_safe_ret_params {
                        false => quote! { #ret },
                        true => quote! { (#ret, (#(#func_param_types,)*)) },
                    };
                    out.extend(quote! {
                        -> impl ::std::future::Future<Output = #ret> {
                            #new_fn(
                                |r_ref: <(#(#func_param_types,)*) as ::rust2go::ToRef>::Ref, slot: *const (), cb: *const ()| {
                                    #[allow(clippy::useless_transmute)]
                                    unsafe {
                                        #path_prefix #c_func_name(
                                            #(::std::mem::transmute(r_ref.#tuple_ids),)*
                                            slot as *const _ as *mut _,
                                            cb as *const _ as *mut _,
                                        )
                                    };
                                },
                                (#(#func_param_names,)*),
                                Self::#callback_name as *const (),
                            )
                        }
                    });
                }
            }
        }
        Ok(out)
    }

    fn to_rs_callback(&self, path_prefix: &TokenStream) -> Result<TokenStream> {
        if let Some(mem_call_id) = self.mem_call_id {
            let fn_name = format_ident!("mem_ffi_handle{}", mem_call_id);
            let drop = if self.ret.is_some() {
                quote! { true }
            } else {
                quote! { false }
            };

            let mut body = None;
            if let Some(ret) = self.ret.as_ref() {
                let resp_ref_ty = ret.to_rust_ref(None);
                let reqs_ty = self.params().iter().map(|p| &p.ty);
                let set_result = if self.drop_safe_ret_params {
                    quote! {
                        ::rust2go_mem_ffi::set_result_for_shared_mut_slot(&slot, (value, *_params));
                    }
                } else {
                    quote! {
                        ::rust2go_mem_ffi::set_result_for_shared_mut_slot(&slot, value);
                    }
                };
                body = Some(quote! {
                    let value_ref = unsafe { &*(response_ptr as *const #resp_ref_ty) };
                    let value: #ret = ::rust2go::FromRef::from_ref(value_ref);

                    let _params = unsafe { Box::from_raw(desc.params_ptr as *mut (#(#reqs_ty,)*)) };

                    let slot = unsafe { ::rust2go_mem_ffi::shared_mut_from_raw(desc.slot_ptr) };
                    #set_result
                });
            }

            return Ok(quote! {
                #[allow(unused_variables)]
                fn #fn_name(response_ptr: usize, desc: ::rust2go_mem_ffi::TaskDesc) -> bool {
                    #body
                    #drop
                }
            });
        }

        let fn_name = format_ident!("{}_cb", self.name);

        match (self.is_async, &self.ret) {
            (true, None) => sbail!("async function must have a return value"),
            (false, None) => {
                // There's no need to generate callback for sync function without callback.
                Ok(TokenStream::default())
            }
            (false, Some(ret)) => {
                // #[no_mangle]
                // unsafe extern "C" fn demo_check_cb(resp: *const binding::DemoResponseRef, slot: *const ()) {
                //     *(slot as *mut Option<DemoResponse>) = Some(::rust2go::FromRef::from_ref(::std::mem::transmute(resp)));
                // }
                let resp_ref_ty = ret.to_rust_ref(Some(path_prefix));
                Ok(quote! {
                    #[allow(clippy::useless_transmute, clippy::transmute_ptr_to_ref)]
                    #[no_mangle]
                    unsafe extern "C" fn #fn_name(resp: *const #resp_ref_ty, slot: *const ()) {
                        *(slot as *mut Option<#ret>) = Some(::rust2go::FromRef::from_ref(::std::mem::transmute(resp)));
                    }
                })
            }
            (true, Some(ret)) => {
                // #[no_mangle]
                // unsafe extern "C" fn demo_check_async_cb(
                //     resp: *const binding::DemoResponseRef,
                //     slot: *const (),
                // ) {
                //     ::rust2go::SlotWriter::<DemoResponse>::from_ptr(slot).write(::rust2go::FromRef::from_ref(::std::mem::transmute(resp)));
                // }
                let resp_ref_ty = ret.to_rust_ref(Some(path_prefix));
                let func_param_types = self.params.iter().map(|p| &p.ty);
                Ok(quote! {
                    #[allow(clippy::useless_transmute, clippy::transmute_ptr_to_ref)]
                    #[no_mangle]
                    unsafe extern "C" fn #fn_name(resp: *const #resp_ref_ty, slot: *const ()) {
                        ::rust2go::SlotWriter::<#ret, ((#(#func_param_types,)*), Vec<u8>)>::from_ptr(slot).write(::rust2go::FromRef::from_ref(::std::mem::transmute(resp)));
                    }
                })
            }
        }
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
