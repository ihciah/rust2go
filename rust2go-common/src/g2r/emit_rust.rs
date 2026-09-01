// Copyright 2024 ihciah. All Rights Reserved.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Result;

use super::G2RTraitRepr;

impl G2RTraitRepr {
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
