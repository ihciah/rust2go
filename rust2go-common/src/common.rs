// Copyright 2024 ihciah. All Rights Reserved.

use crate::{g2r::G2RTraitRepr, r2g::R2GTraitRepr};
use heck::{
    ToKebabCase, ToLowerCamelCase, ToShoutyKebabCase, ToShoutySnakeCase, ToSnakeCase, ToTitleCase,
    ToTrainCase, ToUpperCamelCase,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use rust2go_convert::primitive_by_rust_ident;
use std::collections::HashMap;
use syn::parse::Parser;
use syn::{
    Attribute, Error, Expr, ExprLit, File, FnArg, GenericArgument, Ident, Item, Lit, Meta,
    MetaNameValue, PathArguments, PathSegment, Result, ReturnType, TraitItem, Type, TypeParamBound,
};

pub struct RawRsFile {
    file: File,
}

impl RawRsFile {
    pub fn new<S: AsRef<str>>(src: S) -> Self {
        let src = src.as_ref();
        let mut syntax = syn::parse_file(src).expect("Unable to parse file");
        expand_type_aliases(&mut syntax);
        RawRsFile { file: syntax }
    }

    pub fn go_internal_drop() -> &'static str {
        include_str!("tmpl/internal_drop.h.tmpl")
    }

    pub fn go_shm_include() -> &'static str {
        include_str!("tmpl/shm_queue_meta.h.tmpl")
    }

    pub fn go_shm_ring_init() -> &'static str {
        include_str!("tmpl/shm_ring_init.go.tmpl")
    }

    // The returned mapping is struct OriginalType -> RefType.
    pub fn convert_structs_to_ref(&self) -> Result<(HashMap<Ident, Ident>, TokenStream)> {
        let mut name_mapping = HashMap::new();

        // Add these to generated code to make golang have C structs of string.
        let mut out = quote! {
            #[repr(C)]
            pub struct StringRef {
                pub ptr: *const u8,
                pub len: usize,
            }
            #[repr(C)]
            pub struct ListRef {
                pub ptr: *const (),
                pub len: usize,
            }
        };
        name_mapping.insert(
            Ident::new("String", Span::call_site()),
            Ident::new("StringRef", Span::call_site()),
        );
        name_mapping.insert(
            Ident::new("Vec", Span::call_site()),
            Ident::new("ListRef", Span::call_site()),
        );

        for item in self.file.items.iter() {
            match item {
                // for example, convert
                // pub struct DemoRequest {
                //     pub name: String,
                //     pub age: u8,
                // }
                // to
                // #[repr(C)]
                // pub struct DemoRequestRef {
                //    pub name: StringRef,
                //    pub age: u8,
                // }
                Item::Struct(s) => {
                    let struct_name = s.ident.clone();
                    let struct_name_ref = format_ident!("{}Ref", struct_name);
                    name_mapping.insert(struct_name, struct_name_ref.clone());
                    let mut field_names = Vec::with_capacity(s.fields.len());
                    let mut field_types = Vec::with_capacity(s.fields.len());
                    for field in s.fields.iter() {
                        let field_name = field
                            .clone()
                            .ident
                            .ok_or_else(|| serr!("only named fields are supported"))?;
                        let field_type = ParamType::try_from(&field.ty)?;
                        field_names.push(field_name);
                        field_types.push(field_type.to_rust_ref(None));
                    }
                    out.extend(quote! {
                        #[repr(C)]
                        pub struct #struct_name_ref {
                            #(pub #field_names: #field_types,)*
                        }
                    });
                }
                _ => continue,
            }
        }
        Ok((name_mapping, out))
    }

    // go structs define and newStruct/refStruct function impl.
    pub fn convert_structs_to_go(
        &self,
        levels: &HashMap<Ident, u8>,
        go118: bool,
    ) -> Result<String> {
        const GO118CODE: &str = include_str!("tmpl/structs_go118.go.tmpl");
        const GO121CODE: &str = include_str!("tmpl/structs_go121.go.tmpl");
        const GO_COMMON: &str = include_str!("tmpl/structs_common.go.tmpl");

        let mut out = if go118 {
            GO118CODE.to_string()
        } else {
            GO121CODE.to_string()
        } + GO_COMMON;
        for item in self.file.items.iter() {
            match item {
                // for example, convert
                // pub struct DemoRequest {
                //     pub name: String,
                //     pub age: u8,
                // }
                // to
                // type DemoRequest struct {
                //     name String
                //     age uint8
                // }
                // func newDemoRequest(p C.DemoRequestRef) DemoRequest {
                //     return DemoRequest {
                //         name: newString(p.name),
                //         age: uint8(p.age),
                //     }
                // }
                // func refDemoRequest(p DemoRequest) C.DemoRequestRef {
                //     return C.DemoRequestRef {
                //         name: refString(p.name),
                //         age: C.uint8_t(p.age),
                //     }
                // }
                Item::Struct(s) => {
                    let go_struct_tag = Self::go_struct_tag(&s.attrs)?;
                    let struct_name = s.ident.to_string();
                    out.push_str(&format!("type {struct_name} struct {{\n"));
                    for field in s.fields.iter() {
                        let field_name = field
                            .ident
                            .as_ref()
                            .ok_or_else(|| serr!("only named fields are supported"))?
                            .to_string();
                        let field_type = ParamType::try_from(&field.ty)?;
                        out.push_str(&format!(
                            "    {} {} {}\n",
                            field_name,
                            field_type.to_go(),
                            Self::gen_tag(&field_name, &go_struct_tag)?
                        ));
                    }
                    out.push_str("}\n");

                    // newStruct
                    out.push_str(&format!(
                        "func new{struct_name}(p C.{struct_name}Ref) {struct_name}{{\nreturn {struct_name}{{\n"
                    ));
                    for field in s.fields.iter() {
                        let field_name = field.ident.as_ref().unwrap().to_string();
                        let field_type = ParamType::try_from(&field.ty)?;
                        let (new_f, _) = field_type.c_to_go_field_converter(levels);
                        out.push_str(&format!("{field_name}: {new_f}(p.{field_name}),\n",));
                    }
                    out.push_str("}\n}\n");

                    // ownStruct
                    out.push_str(&format!(
                        "func own{struct_name}(p C.{struct_name}Ref) {struct_name}{{\nreturn {struct_name}{{\n"
                    ));
                    for field in s.fields.iter() {
                        let field_name = field.ident.as_ref().unwrap().to_string();
                        let field_type = ParamType::try_from(&field.ty)?;
                        let own_f = field_type.c_to_go_field_converter_owned();
                        out.push_str(&format!("{field_name}: {own_f}(p.{field_name}),\n",));
                    }
                    out.push_str("}\n}\n");

                    // cntStruct
                    let level = *levels.get(&s.ident).unwrap();
                    out.push_str(&format!(
                        "func cnt{struct_name}(s *{struct_name}, cnt *uint) [0]C.{struct_name}Ref {{\n"
                    ));
                    let mut used = false;
                    if level == 2 {
                        for field in s.fields.iter() {
                            let field_name = field.ident.as_ref().unwrap().to_string();
                            let field_type = ParamType::try_from(&field.ty)?;
                            let (counter_f, level) = field_type.go_to_c_field_counter(levels);
                            if level == 2 {
                                out.push_str(&format!("{counter_f}(&s.{field_name}, cnt)\n"));
                                used = true;
                            }
                        }
                    }
                    if !used {
                        out.push_str("_ = s\n_ = cnt\n");
                    }
                    out.push_str(&format!("return [0]C.{struct_name}Ref{{}}\n"));
                    out.push_str("}\n");

                    // refStruct
                    out.push_str(&format!(
                        "func ref{struct_name}(p *{struct_name}, buffer *[]byte) C.{struct_name}Ref{{\nreturn C.{struct_name}Ref{{\n"
                    ));
                    for field in s.fields.iter() {
                        let field_name = field.ident.as_ref().unwrap().to_string();
                        let field_type = ParamType::try_from(&field.ty)?;
                        let (ref_f, _) = field_type.go_to_c_field_converter(levels);
                        out.push_str(&format!(
                            "{field_name}: {ref_f}(&p.{field_name}, buffer),\n",
                        ));
                    }
                    out.push_str("}\n}\n");
                }
                _ => continue,
            }
        }
        Ok(out)
    }

    pub fn convert_r2g_trait(&self) -> Result<Vec<R2GTraitRepr>> {
        let out: Vec<R2GTraitRepr> = self
            .file
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Trait(t)
                    if t.attrs
                        .iter()
                        .any(|attr| attr.meta.path().segments.last().unwrap().ident == "r2g") =>
                {
                    Some(t)
                }
                _ => None,
            })
            .map(|item_trait| item_trait.try_into())
            .collect::<Result<Vec<R2GTraitRepr>>>()?;
        Ok(out)
    }

    pub fn convert_g2r_trait(&self) -> Result<Vec<G2RTraitRepr>> {
        let out: Vec<G2RTraitRepr> = self
            .file
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Trait(t)
                    if t.attrs
                        .iter()
                        .any(|attr| attr.meta.path().segments.last().unwrap().ident == "g2r") =>
                {
                    Some(t)
                }
                _ => None,
            })
            .map(|item_trait| item_trait.try_into())
            .collect::<Result<Vec<G2RTraitRepr>>>()?;
        Ok(out)
    }

    // 0->Primitive
    // 1->SimpleWrapper
    // 2->Complex
    pub fn convert_structs_levels(&self) -> Result<HashMap<Ident, u8>> {
        enum Node {
            List(Box<Node>),
            NamedStruct(Ident),
            Primitive,
        }
        fn type_to_node(ty: &Type) -> Result<Node> {
            let seg = type_to_segment(ty)?;
            match classify_ref_field(&seg.ident) {
                RefFieldClass::List => {
                    let inside = match &seg.arguments {
                        syn::PathArguments::AngleBracketed(ga) => match ga.args.last().unwrap() {
                            syn::GenericArgument::Type(ty) => ty,
                            _ => panic!("list generic must be a type"),
                        },
                        _ => panic!("list type must have angle bracketed arguments"),
                    };
                    Ok(Node::List(Box::new(type_to_node(inside)?)))
                }
                RefFieldClass::Primitive => Ok(Node::Primitive),
                _ => Ok(Node::NamedStruct(seg.ident.clone())),
            }
        }
        fn node_level(
            node: &Node,
            items: &HashMap<Ident, Vec<Node>>,
            out: &mut HashMap<Ident, u8>,
        ) -> u8 {
            match node {
                Node::List(inner) => (1 + node_level(inner, items, out)).min(2),
                Node::NamedStruct(ident) if ident.to_string().as_str() == "String" => 1,
                Node::NamedStruct(name) => {
                    if let Some(lv) = out.get(name) {
                        return *lv;
                    }
                    let lv = items
                        .get(name)
                        .map(|nodes| {
                            nodes
                                .iter()
                                .map(|n| node_level(n, items, out))
                                .max()
                                .unwrap_or(0)
                        })
                        .unwrap();
                    out.insert(name.clone(), lv);
                    lv
                }
                Node::Primitive => 0,
            }
        }
        let mut items = HashMap::<Ident, Vec<Node>>::new();
        for item in self.file.items.iter() {
            match item {
                Item::Struct(s) => {
                    let mut fields = Vec::new();
                    for field in &s.fields {
                        fields.push(type_to_node(&field.ty)?);
                    }
                    items.insert(s.ident.clone(), fields);
                }
                _ => continue,
            }
        }

        let mut out = HashMap::new();
        for name in items.keys() {
            let lv = node_level(&Node::NamedStruct(name.clone()), &items, &mut out);
            out.insert(name.clone(), lv);
        }
        out.insert(Ident::new("String", Span::call_site()), 1);
        Ok(out)
    }

    fn is_r2g_struct_tag(attr: &Attribute) -> bool {
        if attr.path().is_ident("r2g_struct_tag") {
            return true;
        }

        let segments: Vec<_> = attr
            .path()
            .segments
            .iter()
            .map(|seg| seg.ident.to_string())
            .collect();

        if segments.len() == 2 && segments[0] == "rust2go" && segments[1] == "r2g_struct_tag" {
            return true;
        }

        false
    }
    fn go_struct_tag(attrs: &[Attribute]) -> Result<Vec<(String, String)>> {
        let mut hash_set = vec![];

        for attr in attrs {
            if Self::is_r2g_struct_tag(attr) {
                let meta_list = match &attr.meta {
                    Meta::List(meta_list) => meta_list,
                    _ => continue,
                };

                let parser = syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated;
                let metas = parser.parse2(meta_list.tokens.clone())?;

                for meta in metas {
                    if let Meta::NameValue(MetaNameValue {
                        path,
                        value:
                            Expr::Lit(ExprLit {
                                lit: Lit::Str(lit_str),
                                ..
                            }),
                        ..
                    }) = meta
                    {
                        if let Some(ident) = path.get_ident() {
                            let key = ident.to_string();
                            let value = lit_str.value();
                            hash_set.push((key, value));
                        }
                    }
                }
            }
        }

        Ok(hash_set)
    }

    fn gen_tag(field_name: &str, tag_list: &[(String, String)]) -> Result<String> {
        let mut tags = vec![];
        for (key, heck_type) in tag_list {
            tags.push(format!(
                "{}:{:?}",
                key,
                Self::heck_field_name(field_name, heck_type)?
            ));
        }
        if tags.is_empty() {
            return Ok(String::new());
        }
        Ok(format!("`{}`", tags.join(" ")))
    }

    fn heck_field_name(field_name: &str, heck_type: &str) -> Result<String> {
        Ok(match heck_type {
            "snake_case" => field_name.to_snake_case(),
            "lowerCamelCase" => field_name.to_lower_camel_case(),
            "UpperCamelCase" => field_name.to_upper_camel_case(),
            "kebab-case" => field_name.to_kebab_case(),
            "SHOUTY_SNAKE_CASE" => field_name.to_shouty_snake_case(),
            "SHOUTY-KEBAB-CASE" => field_name.to_shouty_kebab_case(),
            "Title Case" => field_name.to_title_case(),
            "Train-Case" => field_name.to_train_case(),
            _ => sbail!("unknown heck type"),
        })
    }
}

pub struct Param {
    pub name: Ident,
    pub ty: ParamType,
}

impl Param {
    pub fn ty(&self) -> &ParamType {
        &self.ty
    }
}

pub struct ParamType {
    pub inner: ParamTypeInner,
    pub is_reference: bool,
}

pub enum ParamTypeInner {
    Primitive(Ident),
    Custom(Ident),
    List(Type),
}

impl ToTokens for ParamType {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        if self.is_reference {
            tokens.extend(quote! {&});
        }
        match &self.inner {
            ParamTypeInner::Primitive(ty) => ty.to_tokens(tokens),
            ParamTypeInner::Custom(ty) => ty.to_tokens(tokens),
            ParamTypeInner::List(ty) => ty.to_tokens(tokens),
        }
    }
}

/// Classification of a type for ref-struct field generation. Shared by the
/// CLI codegen (`convert_structs_to_ref`) and the `R2G` derive macro so both
/// paths apply the exact same rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefFieldClass {
    /// Primitives are kept as-is in the ref struct.
    Primitive,
    /// `String` maps to `StringRef`.
    String,
    /// `Vec<T>` and `Option<T>` map to `ListRef`.
    List,
    /// Any other type is a custom (struct) type.
    Custom,
}

/// Classify a type by its (first) path segment ident.
pub fn classify_ref_field(ident: &Ident) -> RefFieldClass {
    match ident.to_string().as_str() {
        "Vec" | "Option" => RefFieldClass::List,
        "String" => RefFieldClass::String,
        name if primitive_by_rust_ident(name).is_some() => RefFieldClass::Primitive,
        _ => RefFieldClass::Custom,
    }
}

// Go converter function name for a primitive type: the prefix (`newC_`,
// `cntC_` or `refC_`) followed by its C type name, e.g. `newC_uint8_t`.
// Returns None for unknown primitives and for primitives without generated
// converters (currently `char`).
fn go_primitive_converter(name: &Ident, prefix: &str) -> Option<String> {
    let info = primitive_by_rust_ident(&name.to_string())?;
    if !info.has_go_converters {
        return None;
    }
    Some(format!("{prefix}{}", info.c_name))
}

impl TryFrom<&Type> for ParamType {
    type Error = Error;

    fn try_from(mut ty: &Type) -> Result<Self> {
        let mut is_reference = false;
        if let Type::Reference(r) = ty {
            is_reference = true;
            ty = &r.elem;
        }

        // TypePath -> ParamType
        let seg = type_to_segment(ty)?;
        let param_type_inner = match classify_ref_field(&seg.ident) {
            RefFieldClass::Primitive => {
                if !seg.arguments.is_none() {
                    sbail!("primitive types with arguments are not supported")
                }
                ParamTypeInner::Primitive(seg.ident.clone())
            }
            RefFieldClass::List => ParamTypeInner::List(ty.clone()),
            _ => {
                if !seg.arguments.is_none() {
                    sbail!("custom types with arguments are not supported")
                }
                ParamTypeInner::Custom(seg.ident.clone())
            }
        };
        Ok(ParamType {
            inner: param_type_inner,
            is_reference,
        })
    }
}

impl ParamType {
    pub fn to_c(&self, with_struct: bool) -> String {
        let struct_ = if with_struct { "struct " } else { "" };
        match &self.inner {
            ParamTypeInner::Primitive(name) => match primitive_by_rust_ident(&name.to_string()) {
                Some(info) => info.c_name.to_string(),
                None => panic!("unrecognized rust primitive type {name}"),
            },
            ParamTypeInner::Custom(c) => format!("{struct_}{c}Ref"),
            ParamTypeInner::List(_) => format!("{struct_}ListRef"),
        }
    }

    pub fn to_go(&self) -> String {
        match &self.inner {
            ParamTypeInner::Primitive(name) => match primitive_by_rust_ident(&name.to_string()) {
                Some(info) => info.go_name.to_string(),
                None => panic!("unrecognized rust primitive type {name}"),
            },
            ParamTypeInner::Custom(c) => {
                let s = c.to_string();
                match s.as_str() {
                    "String" => "string".to_string(),
                    _ => s,
                }
            }
            ParamTypeInner::List(inner) => {
                let seg = type_to_segment(inner).unwrap();
                let inside = match &seg.arguments {
                    syn::PathArguments::AngleBracketed(ga) => match ga.args.last().unwrap() {
                        syn::GenericArgument::Type(ty) => ty,
                        _ => panic!("list generic must be a type"),
                    },
                    _ => panic!("list type must have angle bracketed arguments"),
                };
                format!(
                    "[]{}",
                    ParamType::try_from(inside)
                        .expect("unable to convert list type")
                        .to_go()
                )
            }
        }
    }

    // f: StructRef -> Struct
    pub fn c_to_go_field_converter(&self, mapping: &HashMap<Ident, u8>) -> (String, u8) {
        match &self.inner {
            ParamTypeInner::Primitive(name) => (
                go_primitive_converter(name, "newC_")
                    .unwrap_or_else(|| panic!("unrecognized rust primitive type {name}")),
                0,
            ),
            ParamTypeInner::Custom(c) => (
                format!("new{}", c.to_string().as_str()),
                *mapping.get(c).unwrap(),
            ),
            ParamTypeInner::List(inner) => {
                let seg = type_to_segment(inner).unwrap();
                let inside = match &seg.arguments {
                    syn::PathArguments::AngleBracketed(ga) => match ga.args.last().unwrap() {
                        syn::GenericArgument::Type(ty) => ty,
                        _ => panic!("list generic must be a type"),
                    },
                    _ => panic!("list type must have angle bracketed arguments"),
                };
                let (inner, inner_level) = ParamType::try_from(inside)
                    .expect("unable to convert list type")
                    .c_to_go_field_converter(mapping);
                if inner_level == 0 {
                    (format!("new_list_mapper_primitive({inner})"), 1)
                } else {
                    (format!("new_list_mapper({inner})"), 2.min(inner_level + 1))
                }
            }
        }
    }

    // f: StructRef -> Struct with fully ownership
    pub fn c_to_go_field_converter_owned(&self) -> String {
        match &self.inner {
            ParamTypeInner::Primitive(name) => go_primitive_converter(name, "newC_")
                .unwrap_or_else(|| panic!("unrecognized rust primitive type {name}")),
            ParamTypeInner::Custom(c) => format!("own{}", c.to_string().as_str()),
            ParamTypeInner::List(inner) => {
                let seg = type_to_segment(inner).unwrap();
                let inside = match &seg.arguments {
                    syn::PathArguments::AngleBracketed(ga) => match ga.args.last().unwrap() {
                        syn::GenericArgument::Type(ty) => ty,
                        _ => panic!("list generic must be a type"),
                    },
                    _ => panic!("list type must have angle bracketed arguments"),
                };
                let inner = ParamType::try_from(inside)
                    .expect("unable to convert list type")
                    .c_to_go_field_converter_owned();
                format!("new_list_mapper({inner})")
            }
        }
    }

    pub fn go_to_c_field_counter(&self, mapping: &HashMap<Ident, u8>) -> (String, u8) {
        match &self.inner {
            ParamTypeInner::Primitive(name) => (
                go_primitive_converter(name, "cntC_")
                    .unwrap_or_else(|| panic!("unrecognized rust primitive type {name}")),
                0,
            ),
            ParamTypeInner::Custom(c) => (
                format!("cnt{}", c.to_string().as_str()),
                *mapping.get(c).unwrap(),
            ),
            ParamTypeInner::List(inner) => {
                let seg = type_to_segment(inner).unwrap();
                let inside = match &seg.arguments {
                    syn::PathArguments::AngleBracketed(ga) => match ga.args.last().unwrap() {
                        syn::GenericArgument::Type(ty) => ty,
                        _ => panic!("list generic must be a type"),
                    },
                    _ => panic!("list type must have angle bracketed arguments"),
                };
                let (inner, inner_level) = ParamType::try_from(inside)
                    .expect("unable to convert list type")
                    .go_to_c_field_counter(mapping);
                if inner_level == 0 {
                    (format!("cnt_list_mapper_primitive({inner})"), 1)
                } else {
                    (format!("cnt_list_mapper({inner})"), 2.min(inner_level + 1))
                }
            }
        }
    }

    // f: Struct -> StructRef
    pub fn go_to_c_field_converter(&self, mapping: &HashMap<Ident, u8>) -> (String, u8) {
        match &self.inner {
            ParamTypeInner::Primitive(name) => (
                go_primitive_converter(name, "refC_")
                    .unwrap_or_else(|| panic!("unrecognized rust primitive type {name}")),
                0,
            ),
            ParamTypeInner::Custom(c) => (
                format!("ref{}", c.to_string().as_str()),
                *mapping.get(c).unwrap(),
            ),
            ParamTypeInner::List(inner) => {
                let seg = type_to_segment(inner).unwrap();
                let inside = match &seg.arguments {
                    syn::PathArguments::AngleBracketed(ga) => match ga.args.last().unwrap() {
                        syn::GenericArgument::Type(ty) => ty,
                        _ => panic!("list generic must be a type"),
                    },
                    _ => panic!("list type must have angle bracketed arguments"),
                };
                let (inner, inner_level) = ParamType::try_from(inside)
                    .expect("unable to convert list type")
                    .go_to_c_field_converter(mapping);
                if inner_level == 0 {
                    (format!("ref_list_mapper_primitive({inner})"), 1)
                } else {
                    (format!("ref_list_mapper({inner})"), 2.min(inner_level + 1))
                }
            }
        }
    }

    pub fn to_rust_ref(&self, prefix: Option<&TokenStream>) -> TokenStream {
        match &self.inner {
            ParamTypeInner::Primitive(name) => quote!(#name),
            ParamTypeInner::Custom(name) => {
                let ident = format_ident!("{}Ref", name);
                quote!(#prefix #ident)
            }
            ParamTypeInner::List(_) => {
                let ident = format_ident!("ListRef");
                quote!(#prefix #ident)
            }
        }
    }

    // Same as `to_rust_ref`, but renders the ref type as the associated type
    // `<T as ::rust2go::ToRef>::Ref`. This form also works for type aliases,
    // which cannot be resolved on the proc-macro side.
    // The prefix is unused since the types are resolved in the user crate.
    pub fn to_rust_ref_assoc(&self, prefix: Option<&TokenStream>) -> TokenStream {
        let _ = prefix;
        match &self.inner {
            ParamTypeInner::Primitive(name) => quote!(#name),
            ParamTypeInner::Custom(name) => {
                quote!(<#name as ::rust2go::ToRef>::Ref)
            }
            ParamTypeInner::List(ty) => {
                quote!(<#ty as ::rust2go::ToRef>::Ref)
            }
        }
    }
}

// Expand non-generic type aliases (e.g. `pub type Amount = i64;`) in struct field
// types and trait method signatures, so the codegen only sees concrete types.
fn expand_type_aliases(file: &mut File) {
    // Collect alias definitions. Aliases with generic parameters are not
    // supported and skipped.
    let mut aliases = HashMap::<Ident, Type>::new();
    for item in file.items.iter() {
        if let Item::Type(t) = item {
            if t.generics.params.is_empty() {
                aliases.insert(t.ident.clone(), t.ty.as_ref().clone());
            }
        }
    }
    if aliases.is_empty() {
        return;
    }

    // Resolve alias chains (A -> B -> i64) and detect cycles.
    let mut resolved = HashMap::<Ident, Type>::new();
    for name in aliases.keys() {
        let mut chain = vec![name.clone()];
        let mut cur = aliases.get(name).unwrap().clone();
        while let Some(next_ident) =
            bare_type_ident(&cur).filter(|ident| aliases.contains_key(ident))
        {
            if chain.contains(&next_ident) {
                let mut chain_str = chain
                    .iter()
                    .map(|ident| ident.to_string())
                    .collect::<Vec<_>>();
                chain_str.push(next_ident.to_string());
                panic!("cyclic type alias detected: {}", chain_str.join(" -> "));
            }
            chain.push(next_ident.clone());
            cur = aliases.get(&next_ident).unwrap().clone();
        }
        resolved.insert(name.clone(), cur);
    }
    // Aliases may also be used inside the generic arguments of alias targets
    // (e.g. `type A = Vec<B>;`), so expand the resolved targets too.
    let aliases_snapshot = resolved.clone();
    for ty in resolved.values_mut() {
        expand_type(ty, &aliases_snapshot);
    }

    // Expand alias usages in type positions of struct fields and trait methods.
    for item in file.items.iter_mut() {
        match item {
            Item::Struct(s) => {
                for field in s.fields.iter_mut() {
                    expand_type(&mut field.ty, &resolved);
                }
            }
            Item::Trait(t) => {
                for trait_item in t.items.iter_mut() {
                    if let TraitItem::Fn(f) = trait_item {
                        for arg in f.sig.inputs.iter_mut() {
                            if let FnArg::Typed(pat_type) = arg {
                                expand_type(&mut pat_type.ty, &resolved);
                            }
                        }
                        if let ReturnType::Type(_, ty) = &mut f.sig.output {
                            expand_type(ty, &resolved);
                        }
                    }
                }
            }
            _ => continue,
        }
    }
}

// If the type is a single-segment bare identifier path, return the identifier.
fn bare_type_ident(ty: &Type) -> Option<Ident> {
    let Type::Path(p) = ty else {
        return None;
    };
    if p.qself.is_some() || p.path.leading_colon.is_some() || p.path.segments.len() != 1 {
        return None;
    }
    let seg = p.path.segments.first().unwrap();
    if !seg.arguments.is_none() {
        return None;
    }
    Some(seg.ident.clone())
}

// Recursively replace alias idents in a type tree. Only type positions are
// touched: bare single-segment idents and types inside generic arguments.
fn expand_type(ty: &mut Type, aliases: &HashMap<Ident, Type>) {
    // Replace the whole type if it is a bare alias ident.
    if let Some(ident) = bare_type_ident(ty) {
        if let Some(target) = aliases.get(&ident) {
            *ty = target.clone();
            return;
        }
    }
    // Otherwise expand nested types in generic arguments.
    fn expand_path_args(args: &mut PathArguments, aliases: &HashMap<Ident, Type>) {
        if let PathArguments::AngleBracketed(args) = args {
            for arg in args.args.iter_mut() {
                match arg {
                    GenericArgument::Type(t) => expand_type(t, aliases),
                    GenericArgument::AssocType(t) => expand_type(&mut t.ty, aliases),
                    _ => continue,
                }
            }
        }
    }
    match ty {
        Type::Path(p) => {
            for seg in p.path.segments.iter_mut() {
                expand_path_args(&mut seg.arguments, aliases);
            }
        }
        Type::Reference(r) => expand_type(&mut r.elem, aliases),
        Type::ImplTrait(i) => {
            for bound in i.bounds.iter_mut() {
                if let TypeParamBound::Trait(t) = bound {
                    for seg in t.path.segments.iter_mut() {
                        expand_path_args(&mut seg.arguments, aliases);
                    }
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn type_to_segment(ty: &Type) -> Result<&PathSegment> {
    let field_type = match ty {
        Type::Path(p) => p,
        _ => sbail!("only path types are supported"),
    };
    let path = &field_type.path;
    // Leading colon is not allow
    if path.leading_colon.is_some() {
        sbail!("types with leading colons are not supported");
    }
    // We only accept single-segment path
    if path.segments.len() != 1 {
        sbail!("types with multiple segments are not supported");
    }
    Ok(path.segments.first().unwrap())
}

#[cfg(test)]
mod tests {
    #[test]
    fn type_alias_expansion() {
        let raw = r#"
        pub type Amount = i64;
        pub type Money = Amount;
        pub type Amounts = Vec<Money>;
        pub struct DemoRequest {
            pub amount: Amount,
            pub tips: Vec<Amount>,
            pub money: Money,
            pub amounts: Amounts,
        }
        pub struct DemoResponse {
            pub pass: bool,
        }
        #[::rust2go::r2g]
        pub trait DemoCall {
            fn demo_check(req: DemoRequest, tip: Amount) -> DemoResponse;
            fn demo_list(amounts: Vec<Amount>) -> Money;
            fn demo_check_async(req: DemoRequest) -> impl std::future::Future<Output = DemoResponse>;
        }
        "#;
        let raw_file = super::RawRsFile::new(raw);
        let levels = raw_file.convert_structs_levels().unwrap();
        let go_structs = raw_file.convert_structs_to_go(&levels, false).unwrap();
        // Struct fields must be expanded to the aliased primitive types.
        assert!(go_structs.contains("amount int64"), "{go_structs}");
        assert!(go_structs.contains("tips []int64"), "{go_structs}");
        assert!(go_structs.contains("money int64"), "{go_structs}");
        assert!(go_structs.contains("amounts []int64"), "{go_structs}");
        assert!(!go_structs.contains("Amount"), "{go_structs}");

        // Ref structs must be generated without alias types.
        let (_mapping, ref_structs) = raw_file.convert_structs_to_ref().unwrap();
        let ref_structs = ref_structs.to_string();
        assert!(!ref_structs.contains("Amount"), "{ref_structs}");

        // Trait method params and return types must be expanded too.
        let traits = raw_file.convert_r2g_trait().unwrap();
        let fns = traits.first().unwrap().fns();
        let demo_check = fns.iter().find(|f| f.name() == "demo_check").unwrap();
        assert_eq!(demo_check.params()[1].ty().to_go(), "int64");
        let demo_list = fns.iter().find(|f| f.name() == "demo_list").unwrap();
        assert_eq!(demo_list.params()[0].ty().to_go(), "[]int64");
        assert_eq!(demo_list.ret().unwrap().to_go(), "int64");
        fns.iter().find(|f| f.name() == "demo_check_async").unwrap();
    }

    #[test]
    #[should_panic(expected = "cyclic type alias detected")]
    fn type_alias_cycle() {
        let raw = r#"
        pub type A = B;
        pub type B = A;
        pub struct S {
            pub a: A,
        }
        "#;
        super::RawRsFile::new(raw);
    }

    #[test]
    fn it_works() {
        let raw = r#"
        pub struct DemoRequest {
            pub name: String,
            pub age: u8,
        }
        pub struct DemoResponse {
            pub pass: bool,
        }
        pub trait DemoCall {
            fn demo_check(req: DemoRequest) -> DemoResponse;
            fn demo_check_async(req: DemoRequest) -> impl std::future::Future<Output = DemoResponse>;
        }
        "#;
        let raw_file = super::RawRsFile::new(raw);
        let traits = raw_file.convert_r2g_trait().unwrap();
        let levels = raw_file.convert_structs_levels().unwrap();

        println!(
            "structs gen: {}",
            raw_file.convert_structs_to_go(&levels, false).unwrap()
        );
        for trait_ in traits {
            println!("if gen: {}", trait_.generate_go_interface());
            println!("go export gen: {}", trait_.generate_go_exports(&levels));
        }
        let levels = raw_file.convert_structs_levels().unwrap();
        levels.iter().for_each(|f| println!("{}: {}", f.0, f.1));
    }

    const DEMO_SRC: &str = r#"
    pub struct DemoNested {
        pub id: u32,
        pub score: f64,
    }
    pub struct DemoRequest {
        pub name: String,
        pub age: u8,
        pub tags: Vec<String>,
        pub scores: Vec<u32>,
        pub maybe_count: Option<u32>,
        pub nested: DemoNested,
        pub nested_list: Vec<DemoNested>,
    }
    pub struct DemoResponse {
        pub pass: bool,
    }
    "#;

    fn ident(name: &str) -> proc_macro2::Ident {
        proc_macro2::Ident::new(name, proc_macro2::Span::call_site())
    }

    fn param_type(src: &str) -> super::ParamType {
        let ty: syn::Type = syn::parse_str(src).expect("unable to parse type");
        super::ParamType::try_from(&ty).expect("unable to convert type")
    }

    fn normalize_tokens(tokens: proc_macro2::TokenStream) -> String {
        tokens
            .to_string()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn go_struct_generation_golden() {
        let raw_file = super::RawRsFile::new(DEMO_SRC);
        let levels = raw_file.convert_structs_levels().unwrap();
        let go = raw_file.convert_structs_to_go(&levels, false).unwrap();

        // Go struct definitions with converted field types.
        assert!(go.contains("type DemoNested struct {"), "{go}");
        assert!(go.contains("id uint32"), "{go}");
        assert!(go.contains("score float64"), "{go}");
        assert!(go.contains("type DemoRequest struct {"), "{go}");
        assert!(go.contains("name string"), "{go}");
        assert!(go.contains("age uint8"), "{go}");
        assert!(go.contains("tags []string"), "{go}");
        assert!(go.contains("scores []uint32"), "{go}");
        assert!(go.contains("maybe_count []uint32"), "{go}");
        assert!(go.contains("nested DemoNested"), "{go}");
        assert!(go.contains("nested_list []DemoNested"), "{go}");
        assert!(go.contains("type DemoResponse struct {"), "{go}");
        assert!(go.contains("pass bool"), "{go}");

        // newStruct: C ref -> Go struct converters.
        assert!(
            go.contains("func newDemoRequest(p C.DemoRequestRef) DemoRequest{"),
            "{go}"
        );
        assert!(go.contains("return DemoRequest{"), "{go}");
        assert!(go.contains("name: newString(p.name),"), "{go}");
        assert!(go.contains("age: newC_uint8_t(p.age),"), "{go}");
        assert!(
            go.contains("tags: new_list_mapper(newString)(p.tags),"),
            "{go}"
        );
        assert!(
            go.contains("scores: new_list_mapper_primitive(newC_uint32_t)(p.scores),"),
            "{go}"
        );
        assert!(
            go.contains("maybe_count: new_list_mapper_primitive(newC_uint32_t)(p.maybe_count),"),
            "{go}"
        );
        assert!(go.contains("nested: newDemoNested(p.nested),"), "{go}");
        assert!(
            go.contains("nested_list: new_list_mapper_primitive(newDemoNested)(p.nested_list),"),
            "{go}"
        );
        assert!(
            go.contains("func newDemoNested(p C.DemoNestedRef) DemoNested{"),
            "{go}"
        );
        assert!(go.contains("id: newC_uint32_t(p.id),"), "{go}");
        assert!(go.contains("score: newC_double(p.score),"), "{go}");

        // ownStruct: C ref -> Go struct with full ownership.
        assert!(
            go.contains("func ownDemoRequest(p C.DemoRequestRef) DemoRequest{"),
            "{go}"
        );
        assert!(go.contains("name: ownString(p.name),"), "{go}");
        assert!(
            go.contains("tags: new_list_mapper(ownString)(p.tags),"),
            "{go}"
        );

        // cntStruct: only level-2 fields are counted.
        assert!(
            go.contains("func cntDemoRequest(s *DemoRequest, cnt *uint) [0]C.DemoRequestRef {"),
            "{go}"
        );
        assert!(
            go.contains("cnt_list_mapper(cntString)(&s.tags, cnt)"),
            "{go}"
        );
        assert!(
            go.contains("func cntDemoNested(s *DemoNested, cnt *uint) [0]C.DemoNestedRef {"),
            "{go}"
        );

        // refStruct: Go struct -> C ref converters.
        assert!(
            go.contains("func refDemoRequest(p *DemoRequest, buffer *[]byte) C.DemoRequestRef{"),
            "{go}"
        );
        assert!(go.contains("return C.DemoRequestRef{"), "{go}");
        assert!(go.contains("name: refString(&p.name, buffer),"), "{go}");
        assert!(go.contains("age: refC_uint8_t(&p.age, buffer),"), "{go}");
        assert!(
            go.contains("tags: ref_list_mapper(refString)(&p.tags, buffer),"),
            "{go}"
        );
        assert!(
            go.contains("scores: ref_list_mapper_primitive(refC_uint32_t)(&p.scores, buffer),"),
            "{go}"
        );
        assert!(
            go.contains("nested: refDemoNested(&p.nested, buffer),"),
            "{go}"
        );
        assert!(
            go.contains(
                "nested_list: ref_list_mapper_primitive(refDemoNested)(&p.nested_list, buffer),"
            ),
            "{go}"
        );

        // Common helpers preamble (go1.21+ variant).
        assert!(
            go.contains("func newString(s_ref C.StringRef) string {"),
            "{go}"
        );
        assert!(go.contains("unsafe.StringData"), "{go}");
        assert!(
            go.contains("func ownString(s_ref C.StringRef) string {"),
            "{go}"
        );
        assert!(
            go.contains("func new_list_mapper[T1, T2 any](f func(T1) T2) func(C.ListRef) []T2 {"),
            "{go}"
        );

        // go1.18 variant emits the reflect-based string helpers instead.
        let go118 = raw_file.convert_structs_to_go(&levels, true).unwrap();
        assert!(
            go118.contains("func unsafeString(ptr *byte, length int) string {"),
            "{go118}"
        );
        assert!(
            go118.contains("func unsafeStringData(s string) *byte {"),
            "{go118}"
        );
    }

    #[test]
    fn ref_struct_generation() {
        let raw_file = super::RawRsFile::new(DEMO_SRC);
        let (mapping, ref_structs) = raw_file.convert_structs_to_ref().unwrap();

        // Name mapping: OriginalType -> RefType.
        assert_eq!(mapping[&ident("String")].to_string(), "StringRef");
        assert_eq!(mapping[&ident("Vec")].to_string(), "ListRef");
        assert_eq!(mapping[&ident("DemoNested")].to_string(), "DemoNestedRef");
        assert_eq!(mapping[&ident("DemoRequest")].to_string(), "DemoRequestRef");
        assert_eq!(
            mapping[&ident("DemoResponse")].to_string(),
            "DemoResponseRef"
        );

        // Generated #[repr(C)] ref structs.
        let norm = normalize_tokens(ref_structs);
        assert!(norm.contains("pub struct StringRef {"), "{norm}");
        assert!(norm.contains("pub struct ListRef {"), "{norm}");
        assert!(norm.contains("pub struct DemoNestedRef {"), "{norm}");
        assert!(norm.contains("pub struct DemoRequestRef {"), "{norm}");
        assert!(norm.contains("pub struct DemoResponseRef {"), "{norm}");
        assert!(norm.contains("name : StringRef"), "{norm}");
        assert!(norm.contains("age : u8"), "{norm}");
        assert!(norm.contains("tags : ListRef"), "{norm}");
        assert!(norm.contains("scores : ListRef"), "{norm}");
        assert!(norm.contains("maybe_count : ListRef"), "{norm}");
        assert!(norm.contains("nested : DemoNestedRef"), "{norm}");
        assert!(norm.contains("nested_list : ListRef"), "{norm}");
        assert!(norm.contains("id : u32"), "{norm}");
        assert!(norm.contains("score : f64"), "{norm}");
        assert!(norm.contains("pass : bool"), "{norm}");
    }

    #[test]
    fn struct_levels() {
        let raw_file = super::RawRsFile::new(DEMO_SRC);
        let levels = raw_file.convert_structs_levels().unwrap();
        // All-primitive struct.
        assert_eq!(levels.get(&ident("DemoNested")), Some(&0));
        // Struct with String/Vec<String> fields.
        assert_eq!(levels.get(&ident("DemoRequest")), Some(&2));
        // Single bool field.
        assert_eq!(levels.get(&ident("DemoResponse")), Some(&0));
        // String is always registered as level 1.
        assert_eq!(levels.get(&ident("String")), Some(&1));
    }

    #[test]
    fn go_struct_tag_generation() {
        let raw = r#"
        #[r2g_struct_tag(json = "snake_case")]
        pub struct TaggedUser {
            pub user_name: String,
            pub login_count: u32,
        }
        "#;
        let raw_file = super::RawRsFile::new(raw);
        let levels = raw_file.convert_structs_levels().unwrap();
        let go = raw_file.convert_structs_to_go(&levels, false).unwrap();
        assert!(go.contains("type TaggedUser struct {"), "{go}");
        assert!(go.contains("user_name string `json:\"user_name\"`"), "{go}");
        assert!(
            go.contains("login_count uint32 `json:\"login_count\"`"),
            "{go}"
        );
    }

    #[test]
    fn param_type_conversions() {
        // Primitive.
        let pt = param_type("u8");
        assert!(!pt.is_reference);
        assert!(matches!(pt.inner, super::ParamTypeInner::Primitive(_)));
        assert_eq!(pt.to_go(), "uint8");
        assert_eq!(pt.to_c(false), "uint8_t");
        assert_eq!(pt.to_c(true), "uint8_t");
        assert_eq!(pt.to_rust_ref(None).to_string(), "u8");

        // Reference to primitive.
        let pt = param_type("&i64");
        assert!(pt.is_reference);
        assert_eq!(pt.to_go(), "int64");
        assert_eq!(pt.to_c(false), "int64_t");

        // Other primitives.
        assert_eq!(param_type("usize").to_go(), "uint");
        assert_eq!(param_type("usize").to_c(false), "uintptr_t");
        assert_eq!(param_type("isize").to_go(), "int");
        assert_eq!(param_type("isize").to_c(false), "intptr_t");
        assert_eq!(param_type("f32").to_go(), "float32");
        assert_eq!(param_type("f32").to_c(false), "float");
        assert_eq!(param_type("f64").to_go(), "float64");
        assert_eq!(param_type("f64").to_c(false), "double");
        assert_eq!(param_type("bool").to_go(), "bool");
        assert_eq!(param_type("bool").to_c(false), "bool");
        assert_eq!(param_type("char").to_go(), "rune");
        assert_eq!(param_type("char").to_c(false), "uint32_t");

        // String.
        let pt = param_type("String");
        assert!(matches!(pt.inner, super::ParamTypeInner::Custom(_)));
        assert_eq!(pt.to_go(), "string");
        assert_eq!(pt.to_c(false), "StringRef");
        assert_eq!(pt.to_c(true), "struct StringRef");
        assert_eq!(pt.to_rust_ref(None).to_string(), "StringRef");

        // Custom struct type.
        let pt = param_type("DemoNested");
        assert!(matches!(pt.inner, super::ParamTypeInner::Custom(_)));
        assert_eq!(pt.to_go(), "DemoNested");
        assert_eq!(pt.to_c(false), "DemoNestedRef");
        assert_eq!(pt.to_c(true), "struct DemoNestedRef");
        assert_eq!(pt.to_rust_ref(None).to_string(), "DemoNestedRef");

        // Vec / Option list types.
        let pt = param_type("Vec<u32>");
        assert!(matches!(pt.inner, super::ParamTypeInner::List(_)));
        assert_eq!(pt.to_go(), "[]uint32");
        assert_eq!(pt.to_c(false), "ListRef");
        assert_eq!(pt.to_c(true), "struct ListRef");
        assert_eq!(pt.to_rust_ref(None).to_string(), "ListRef");
        assert_eq!(param_type("Option<String>").to_go(), "[]string");
        assert_eq!(param_type("Vec<Vec<u8>>").to_go(), "[][]uint8");
        assert_eq!(param_type("Option<DemoNested>").to_go(), "[]DemoNested");

        // Associated-type ref form resolves via rust2go::ToRef.
        let norm = normalize_tokens(param_type("String").to_rust_ref_assoc(None));
        assert!(norm.contains("rust2go :: ToRef"), "{norm}");
    }

    #[test]
    fn param_type_field_converters() {
        let levels = super::RawRsFile::new("pub struct DemoNested { pub id: u32, }")
            .convert_structs_levels()
            .unwrap();

        // Primitive.
        let pt = param_type("u16");
        assert_eq!(
            pt.c_to_go_field_converter(&levels),
            ("newC_uint16_t".to_string(), 0)
        );
        assert_eq!(pt.c_to_go_field_converter_owned(), "newC_uint16_t");
        assert_eq!(
            pt.go_to_c_field_counter(&levels),
            ("cntC_uint16_t".to_string(), 0)
        );
        assert_eq!(
            pt.go_to_c_field_converter(&levels),
            ("refC_uint16_t".to_string(), 0)
        );

        // String (level 1 custom).
        let pt = param_type("String");
        assert_eq!(
            pt.c_to_go_field_converter(&levels),
            ("newString".to_string(), 1)
        );
        assert_eq!(pt.c_to_go_field_converter_owned(), "ownString");
        assert_eq!(
            pt.go_to_c_field_counter(&levels),
            ("cntString".to_string(), 1)
        );
        assert_eq!(
            pt.go_to_c_field_converter(&levels),
            ("refString".to_string(), 1)
        );

        // Level-0 custom struct.
        let pt = param_type("DemoNested");
        assert_eq!(
            pt.c_to_go_field_converter(&levels),
            ("newDemoNested".to_string(), 0)
        );
        assert_eq!(pt.c_to_go_field_converter_owned(), "ownDemoNested");
        assert_eq!(
            pt.go_to_c_field_counter(&levels),
            ("cntDemoNested".to_string(), 0)
        );
        assert_eq!(
            pt.go_to_c_field_converter(&levels),
            ("refDemoNested".to_string(), 0)
        );

        // List of primitives uses the primitive mappers (level 1).
        let pt = param_type("Vec<u8>");
        assert_eq!(
            pt.c_to_go_field_converter(&levels),
            ("new_list_mapper_primitive(newC_uint8_t)".to_string(), 1)
        );
        assert_eq!(
            pt.c_to_go_field_converter_owned(),
            "new_list_mapper(newC_uint8_t)"
        );
        assert_eq!(
            pt.go_to_c_field_counter(&levels),
            ("cnt_list_mapper_primitive(cntC_uint8_t)".to_string(), 1)
        );
        assert_eq!(
            pt.go_to_c_field_converter(&levels),
            ("ref_list_mapper_primitive(refC_uint8_t)".to_string(), 1)
        );

        // Option<String> wraps a level-1 type, becoming level 2.
        let pt = param_type("Option<String>");
        assert_eq!(
            pt.c_to_go_field_converter(&levels),
            ("new_list_mapper(newString)".to_string(), 2)
        );
        assert_eq!(
            pt.c_to_go_field_converter_owned(),
            "new_list_mapper(ownString)"
        );
        assert_eq!(
            pt.go_to_c_field_counter(&levels),
            ("cnt_list_mapper(cntString)".to_string(), 2)
        );
        assert_eq!(
            pt.go_to_c_field_converter(&levels),
            ("ref_list_mapper(refString)".to_string(), 2)
        );

        // List of level-0 structs still uses the primitive mappers (level 1).
        let pt = param_type("Vec<DemoNested>");
        assert_eq!(
            pt.c_to_go_field_converter(&levels),
            ("new_list_mapper_primitive(newDemoNested)".to_string(), 1)
        );
        assert_eq!(
            pt.go_to_c_field_converter(&levels),
            ("ref_list_mapper_primitive(refDemoNested)".to_string(), 1)
        );
    }
}
