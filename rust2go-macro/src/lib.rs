use proc_macro::TokenStream;
use quote::quote;
use rust2go_common::{raw_file::TraitRepr, sbail};
use syn::{parse_macro_input, DeriveInput, Ident};

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
            _ => {
                let ref_type = Ident::new(&format!("{name}Ref"), first_seg.ident.span());
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

#[proc_macro_attribute]
pub fn r2g(attr: TokenStream, item: TokenStream) -> TokenStream {
    let binding_path = if attr.is_empty() {
        None
    } else {
        match syn::parse::<syn::Path>(attr) {
            Ok(path) => Some(path),
            Err(e) => return TokenStream::from(e.to_compile_error()),
        }
    };
    syn::parse::<syn::ItemTrait>(item)
        .and_then(|trat| r2g_trait(binding_path, trat))
        .unwrap_or_else(|e| TokenStream::from(e.to_compile_error()))
}

fn r2g_trait(
    binding_path: Option<syn::Path>,
    mut trat: syn::ItemTrait,
) -> syn::Result<TokenStream> {
    let trat_repr = TraitRepr::try_from(&trat)?;

    // remove attributes of all functions
    trat.items
        .iter_mut()
        .filter_map(|trat_fn| match trat_fn {
            syn::TraitItem::Fn(f) => Some(f),
            _ => None,
        })
        .for_each(|f| {
            f.attrs.clear();
        });

    // for all functions with #[drop_safe_ret], change the return type.
    for (fn_repr, trat_fn) in trat_repr
        .fns()
        .iter()
        .zip(trat.items.iter_mut())
        .filter(|(fn_repr, _)| fn_repr.drop_safe_ret_params())
    {
        match trat_fn {
            syn::TraitItem::Fn(f) => {
                let orig = match fn_repr.ret() {
                    None => quote! { () },
                    Some(ret) => quote! { #ret },
                };
                let tys = fn_repr.params().iter().map(|p| p.ty());

                f.sig.asyncness = None;
                f.sig.output = syn::parse_quote! { -> impl ::std::future::Future<Output = (#orig, (#(#tys,)*))> };
            }
            _ => sbail!("only fn is supported"),
        }
    }

    // for all functions with safe=false, add unsafe
    for (_, trat_fn) in trat_repr
        .fns()
        .iter()
        .zip(trat.items.iter_mut())
        .filter(|(fn_repr, _)| !fn_repr.safe())
    {
        match trat_fn {
            syn::TraitItem::Fn(f) => {
                f.sig.unsafety = Some(syn::token::Unsafe::default());
            }
            _ => sbail!("only fn is supported"),
        }
    }

    let mut out = quote! {#trat};
    out.extend(trat_repr.generate_rs(binding_path.as_ref())?);
    Ok(out.into())
}
