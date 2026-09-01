// Copyright 2024 ihciah. All Rights Reserved.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Ident, Path, Result, Token};

use super::{R2GFnRepr, R2GTraitRepr};

impl R2GTraitRepr {
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
                let resp_ref_ty = ret.to_rust_ref_assoc(None);
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
                let resp_ref_ty = ret.to_rust_ref_assoc(Some(path_prefix));
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
                let resp_ref_ty = ret.to_rust_ref_assoc(Some(path_prefix));
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
