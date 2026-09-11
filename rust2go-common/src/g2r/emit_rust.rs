// Copyright 2024 ihciah. All Rights Reserved.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Result;

use super::G2RTraitRepr;

impl G2RTraitRepr {
    // Generate rust impl.
    pub fn generate_rs(&self) -> Result<TokenStream> {
        let trait_name = &self.name;
        let stateful = self.stateful;
        let instance_name = format_ident!("{}_INSTANCE", trait_name.to_string().to_uppercase());
        let impl_struct_name = format_ident!("{}Impl", trait_name);
        let not_registered_msg = format!(
            "rust2go: {impl_struct_name}::register() was not called \
             before Go invoked {trait_name}"
        );

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

            let instance_expr = stateful.then(|| {
                quote! {
                    let _internal_instance = #instance_name.get().unwrap_or_else(|| {
                        ::std::eprintln!(#not_registered_msg);
                        ::std::process::abort()
                    });
                }
            });
            let call_expr = if stateful {
                quote! { _internal_instance.#f_name(#(#param_names),*) }
            } else {
                quote! { <Self as #trait_name>::#f_name(#(#param_names),*) }
            };

            let bottom = if f.ret.is_some() {
                quote! {
                    let _internal_out = #call_expr;
                    let (_internal_buf, _internal_out_ref) = ::rust2go::ToRef::calc_ref(&_internal_out);

                    let _internal_boxed_storage = ::std::boxed::Box::new((_internal_out, _internal_out_ref, _internal_buf));
                    let ret_ptr = &_internal_boxed_storage.as_ref().1 as *const _ as *const ();
                    let drop_ptr = ::std::boxed::Box::leak(_internal_boxed_storage as ::std::boxed::Box<dyn ::std::any::Any>) as *mut dyn ::std::any::Any as *mut ();

                    *_internal_slot = [ret_ptr, drop_ptr];
                }
            } else {
                quote! {
                    #call_expr;
                }
            };

            fn_entries.push(quote! {
                #[no_mangle]
                unsafe extern "C" fn #cf_name(#slot_expr #params_expr) {
                    #instance_expr
                    #(#params)*
                    #bottom
                }
            });
        }

        let registry_static = stateful.then(|| {
            quote! {
                static #instance_name: ::std::sync::OnceLock<
                    ::std::sync::Arc<dyn #trait_name + Send + Sync>,
                > = ::std::sync::OnceLock::new();
            }
        });
        let register_fn = stateful.then(|| {
            quote! {
                /// Register the trait implementation. Must be called once before Go
                /// invokes any method of the trait; returns Err with the passed
                /// instance if an implementation is already registered.
                pub fn register<T: #trait_name + Send + Sync + 'static>(
                    impl_: T,
                ) -> ::std::result::Result<(), ::std::sync::Arc<dyn #trait_name + Send + Sync>>
                {
                    #instance_name.set(::std::sync::Arc::new(impl_))
                }
            }
        });

        Ok(quote! {
            pub struct #impl_struct_name;
            #registry_static
            impl #impl_struct_name {
                #register_fn
                #(#fn_entries)*
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use syn::ItemTrait;

    use super::*;

    fn repr_of(src: &str) -> G2RTraitRepr {
        let item: ItemTrait = syn::parse_str(src).expect("trait should parse");
        G2RTraitRepr::try_from(&item).expect("trait should convert")
    }

    #[test]
    fn stateful_codegen_has_registry_and_instance_dispatch() {
        let src = "pub trait G2RCall {
            fn demo_log(&self, name: String, age: u8);
            fn demo_check(&self, age: u8) -> String;
        }";
        let out = repr_of(src).generate_rs().unwrap().to_string();
        for needle in [
            "OnceLock",
            "Arc",
            "G2RCALL_INSTANCE",
            "register",
            "Send",
            "Sync",
            "abort",
            "_internal_instance",
            "c_G2RCall_demo_log",
            "c_G2RCall_demo_check",
        ] {
            assert!(out.contains(needle), "missing {needle} in {out}");
        }
        // Stateless-style static dispatch must not appear.
        assert!(!out.contains("<Self as"), "{out}");
    }

    #[test]
    fn stateless_codegen_matches_snapshot() {
        let src = "pub trait G2RCall {
            fn demo_log(name: String, age: u8);
            fn demo_check(age: u8) -> String;
        }";
        let expected_src = r#"
            pub struct G2RCallImpl;
            impl G2RCallImpl {
                #[no_mangle]
                unsafe extern "C" fn c_G2RCall_demo_log(_internal_params: *const *const ()) {
                    let name = _internal_params.offset(0isize).read() as *const _;
                    let name = ::rust2go::FromRef::from_ref(unsafe { &*name });
                    let age = _internal_params.offset(1isize).read() as *const _;
                    let age = ::rust2go::FromRef::from_ref(unsafe { &*age });
                    <Self as G2RCall>::demo_log(name, age);
                }
                #[no_mangle]
                unsafe extern "C" fn c_G2RCall_demo_check(
                    _internal_slot: *mut [*const (); 2],
                    _internal_params: *const *const ()
                ) {
                    let age = _internal_params.offset(0isize).read() as *const _;
                    let age = ::rust2go::FromRef::from_ref(unsafe { &*age });
                    let _internal_out = <Self as G2RCall>::demo_check(age);
                    let (_internal_buf, _internal_out_ref) = ::rust2go::ToRef::calc_ref(&_internal_out);

                    let _internal_boxed_storage = ::std::boxed::Box::new((_internal_out, _internal_out_ref, _internal_buf));
                    let ret_ptr = &_internal_boxed_storage.as_ref().1 as *const _ as *const ();
                    let drop_ptr = ::std::boxed::Box::leak(_internal_boxed_storage as ::std::boxed::Box<dyn ::std::any::Any>) as *mut dyn ::std::any::Any as *mut ();

                    *_internal_slot = [ret_ptr, drop_ptr];
                }
            }
        "#;
        // Compare token trees ignoring punct joint spacing: quote!-generated
        // and syn-parsed streams attach different spacing to puncts (e.g.
        // `&*x` vs `& * x`), which `TokenStream::to_string` would render
        // differently despite identical token sequences.
        fn normalize(ts: TokenStream) -> String {
            use proc_macro2::{Delimiter, TokenTree};
            ts.into_iter()
                .map(|tt| match tt {
                    TokenTree::Ident(i) => i.to_string(),
                    TokenTree::Punct(p) => p.as_char().to_string(),
                    TokenTree::Literal(l) => l.to_string(),
                    TokenTree::Group(g) => {
                        let (open, close) = match g.delimiter() {
                            Delimiter::Parenthesis => ("(", ")"),
                            Delimiter::Brace => ("{", "}"),
                            Delimiter::Bracket => ("[", "]"),
                            Delimiter::None => ("", ""),
                        };
                        format!("{open} {} {close}", normalize(g.stream()))
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
        let actual = normalize(repr_of(src).generate_rs().unwrap());
        let expected = normalize(syn::parse_str::<TokenStream>(expected_src).unwrap());
        assert_eq!(actual, expected);
    }
}
