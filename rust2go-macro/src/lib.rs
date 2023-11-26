use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

/// An proc_macro_derive macro that implements `GetRef` and `GetOwned` for a type.
#[proc_macro_derive(R2GCvt)]
pub fn r2g_cvt(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    // Skip derive when the type has generics or not end with "Ref".
    if !input.generics.params.is_empty() {
        return TokenStream::default();
    }
    let ref_type = input.ident;
    let ref_type_str = ref_type.to_string();
    let original_type = match ref_type_str.strip_suffix("Ref") {
        Some(o) => syn::Ident::new(o, ref_type.span()),
        None => return TokenStream::default(),
    };
    let data = match input.data {
        syn::Data::Struct(d) => d,
        _ => return TokenStream::default(),
    };

    let mut get_refs = Vec::with_capacity(data.fields.len());
    let mut get_owneds = Vec::with_capacity(data.fields.len());
    for field in data.fields.iter() {
        let name = field.ident.as_ref().unwrap();
        get_refs.push(quote! {#name: ::rust2go::RefConvertion::get_ref(&owned.#name)});
        get_owneds.push(quote! {#name: ::rust2go::RefConvertion::get_owned(&self.#name)});
    }

    let expanded = quote! {
        unsafe impl ::rust2go::RefConvertion for #ref_type {
            type Owned = super::#original_type;
            fn get_ref(owned: &Self::Owned) -> Self {
                Self {
                    #(#get_refs),*
                }
            }
            unsafe fn get_owned(&self) -> Self::Owned {
                Self::Owned {
                    #(#get_owneds),*
                }
            }
        }
    };
    TokenStream::from(expanded)
}
