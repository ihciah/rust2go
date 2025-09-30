// Copyright 2024 ihciah. All Rights Reserved.

use proc_macro::TokenStream;
use quote::{format_ident, quote, ToTokens};
use rust2go_common::{g2r::G2RTraitRepr, r2g::R2GTraitRepr, sbail};
use syn::{parse::Parser, parse_macro_input, DeriveInput, Ident};

#[proc_macro_derive(R2G)]
pub fn r2g_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    // Skip derive when the type has generics.
    if !input.generics.params.is_empty() {
        return TokenStream::default();
    }
    // Skip derive when the type is not struct.
    let data = match input.data {
        syn::Data::Struct(d) => d,
        _ => return TokenStream::default(),
    };
    let attrs = input.attrs;
    let type_name = input.ident;
    let type_name_str = type_name.to_string();

    let ref_type_name = Ident::new(&format!("{type_name_str}Ref"), type_name.span());
    let mut ref_fields = Vec::with_capacity(data.fields.len());
    for field in data.fields.iter() {
        let name = field.ident.as_ref().unwrap();
        let ty = &field.ty;
        let syn::Type::Path(path) = ty else {
            return TokenStream::default();
        };
        let Some(first_seg) = path.path.segments.first() else {
            return TokenStream::default();
        };
        match first_seg.ident.to_string().as_str() {
            "Vec" => {
                ref_fields.push(quote! {#name: ::rust2go::ListRef});
            }
            "String" => {
                ref_fields.push(quote! {#name: ::rust2go::StringRef});
            }
            "i8" | "i16" | "i32" | "i64" | "isize" | "u8" | "u16" | "u32" | "u64" | "usize"
            | "f32" | "f64" | "bool" | "char" => {
                ref_fields.push(quote! {#name: #ty});
            }
            ty => {
                let ref_type = format_ident!("{ty}Ref");
                ref_fields.push(quote! {#name: #ref_type});
            }
        }
    }

    let mut owned_names = Vec::with_capacity(data.fields.len());
    let mut owned_types = Vec::with_capacity(data.fields.len());
    for field in data.fields.iter() {
        owned_names.push(field.ident.clone().unwrap());
        owned_types.push(field.ty.clone());
    }

    let expanded = quote! {
        #(#attrs)*
        #[repr(C)]
        pub struct #ref_type_name {
            #(#ref_fields),*
        }

        impl ::rust2go::ToRef for #type_name {
            const MEM_TYPE: ::rust2go::MemType = ::rust2go::max_mem_type!(#(#owned_types),*);
            type Ref = #ref_type_name;

            fn to_size(&self, acc: &mut usize) {
                if matches!(Self::MEM_TYPE, ::rust2go::MemType::Complex) {
                    #(self.#owned_names.to_size(acc);)*
                }
            }

            fn to_ref(&self, buffer: &mut ::rust2go::Writer) -> Self::Ref {
                #ref_type_name {
                    #(#owned_names: ::rust2go::ToRef::to_ref(&self.#owned_names, buffer),)*
                }
            }
        }

        impl ::rust2go::FromRef for #type_name {
            type Ref = #ref_type_name;

            fn from_ref(ref_: &Self::Ref) -> Self {
                Self {
                    #(#owned_names: ::rust2go::FromRef::from_ref(&ref_.#owned_names),)*
                }
            }
        }
    };
    TokenStream::from(expanded)
}

fn parse_attrs(attrs: TokenStream) -> (Option<syn::Path>, Option<usize>) {
    let mut binding_path = None;
    let mut queue_size = None;

    type AttributeArgs = syn::punctuated::Punctuated<syn::Meta, syn::Token![,]>;
    if let Ok(attrs) = AttributeArgs::parse_terminated.parse(attrs) {
        for attr in attrs {
            match attr {
                syn::Meta::NameValue(nv) => {
                    if nv.path.is_ident("binding") {
                        binding_path = Some(nv.path);
                    } else if nv.path.is_ident("queue_size") {
                        if let syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Int(litint),
                            ..
                        }) = nv.value
                        {
                            queue_size = Some(litint.base10_parse::<usize>().unwrap());
                        }
                    }
                }
                syn::Meta::Path(p) => {
                    binding_path = Some(p);
                }
                _ => {}
            }
        }
    }
    (binding_path, queue_size)
}

#[proc_macro_attribute]
pub fn r2g(attrs: TokenStream, item: TokenStream) -> TokenStream {
    let (binding_path, queue_size) = parse_attrs(attrs);
    syn::parse::<syn::ItemTrait>(item)
        .and_then(|trat| r2g_trait(binding_path, queue_size, trat))
        .unwrap_or_else(|e| TokenStream::from(e.to_compile_error()))
}

/// Mark only
#[proc_macro_attribute]
pub fn r2g_struct_tag(_attrs: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn g2r(_attrs: TokenStream, item: TokenStream) -> TokenStream {
    syn::parse::<syn::ItemTrait>(item)
        .and_then(g2r_trait)
        .unwrap_or_else(|e| TokenStream::from(e.to_compile_error()))
}

fn g2r_trait(mut trat: syn::ItemTrait) -> syn::Result<TokenStream> {
    let trat_repr = G2RTraitRepr::try_from(&trat)?;

    for trat_fn in trat.items.iter_mut() {
        match trat_fn {
            syn::TraitItem::Fn(f) => {
                // remove attributes of all functions
                f.attrs.clear();
            }
            _ => sbail!("only fn is supported"),
        }
    }

    let mut out = quote! {#trat};
    out.extend(trat_repr.generate_rs()?);
    Ok(out.into())
}

fn r2g_trait(
    binding_path: Option<syn::Path>,
    queue_size: Option<usize>,
    mut trat: syn::ItemTrait,
) -> syn::Result<TokenStream> {
    let trat_repr = R2GTraitRepr::try_from(&trat)?;

    for (fn_repr, trat_fn) in trat_repr.fns().iter().zip(trat.items.iter_mut()) {
        match trat_fn {
            syn::TraitItem::Fn(f) => {
                // remove attributes of all functions
                f.attrs.clear();

                // for shm based oneway call, add unsafe
                if fn_repr.ret().is_none() && !fn_repr.is_async() && fn_repr.mem_call_id().is_some()
                {
                    f.sig.unsafety = Some(syn::token::Unsafe::default());
                }

                // convert async fn return impl future
                if fn_repr.is_async() {
                    let orig = match fn_repr.ret() {
                        None => quote! { () },
                        Some(ret) => quote! { #ret },
                    };
                    let auto_t = match (fn_repr.ret_send(), fn_repr.ret_static()) {
                        (true, true) => quote!( + Send + Sync + 'static),
                        (true, false) => quote!( + Send + Sync),
                        (false, true) => quote!( + 'static),
                        (false, false) => quote!(),
                    };
                    f.sig.asyncness = None;
                    if fn_repr.drop_safe_ret_params() {
                        // for all functions with #[drop_safe_ret], change the return type.
                        let tys = fn_repr.params().iter().map(|p| p.ty());
                        f.sig.output = syn::parse_quote! { -> impl ::std::future::Future<Output = (#orig, (#(#tys,)*))> #auto_t };
                    } else {
                        f.sig.output = syn::parse_quote! { -> impl ::std::future::Future<Output = #orig> #auto_t };
                    }

                    // for all functions with safe=false, add unsafe
                    if !fn_repr.is_safe() {
                        f.sig.unsafety = Some(syn::token::Unsafe::default());
                    }
                }
            }
            _ => sbail!("only fn is supported"),
        }
    }

    let mut out = quote! {#trat};
    out.extend(trat_repr.generate_rs(binding_path.as_ref(), queue_size)?);
    Ok(out.into())
}
