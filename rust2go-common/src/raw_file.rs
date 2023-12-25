use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use std::collections::HashMap;
use syn::{
    Error, File, FnArg, Ident, Item, ItemTrait, Meta, Pat, Path, PathSegment, Result, ReturnType,
    Token, TraitItem, Type,
};

#[macro_export]
macro_rules! serr {
    ($msg:expr) => {
        ::syn::Error::new(::proc_macro2::Span::call_site(), $msg)
    };
}

#[macro_export]
macro_rules! sbail {
    ($msg:expr) => {
        return Err(::syn::Error::new(::proc_macro2::Span::call_site(), $msg))
    };
}

pub struct RawRsFile {
    file: File,
}

impl RawRsFile {
    pub fn new<S: AsRef<str>>(src: S) -> Self {
        let src = src.as_ref();
        let syntax = syn::parse_file(src).expect("Unable to parse file");
        RawRsFile { file: syntax }
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
                        field_types.push(field_type.to_rust_ref());
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
        const GO118CODE: &str = r#"
        // An alternative impl of unsafe.String for go1.18
        func unsafeString(ptr *byte, length int) string {
            sliceHeader := &reflect.SliceHeader{
                Data: uintptr(unsafe.Pointer(ptr)),
                Len:  length,
                Cap:  length,
            }
            return *(*string)(unsafe.Pointer(sliceHeader))
        }

        // An alternative impl of unsafe.StringData for go1.18
        func unsafeStringData(s string) *byte {
            return (*byte)(unsafe.Pointer((*reflect.StringHeader)(unsafe.Pointer(&s)).Data))
        }
        func newString(s_ref C.StringRef) string {
            return unsafeString((*byte)(unsafe.Pointer(s_ref.ptr)), int(s_ref.len))
        }
        func refString(s *string, _buffer *[]byte) C.StringRef {
            return C.StringRef{
                ptr: (*C.uint8_t)(unsafeStringData(*s)),
                len: C.uintptr_t(len(*s)),
            }
        }
        "#;

        const GO121CODE: &str = r#"
        func newString(s_ref C.StringRef) string {
            return unsafe.String((*byte)(unsafe.Pointer(s_ref.ptr)), s_ref.len)
        }
        func refString(s *string, _buffer *[]byte) C.StringRef {
            return C.StringRef{
                ptr: (*C.uint8_t)(unsafe.StringData(*s)),
                len: C.uintptr_t(len(*s)),
            }
        }
        "#;

        let mut out = if go118 {
            GO118CODE.to_string()
        } else {
            GO121CODE.to_string()
        } + r#"
        func cntString(s *string, cnt *uint) {}
        func new_list_mapper[T1, T2 any](f func(T1) T2) func(C.ListRef) []T2 {
            return func(x C.ListRef) []T2 {
                input := unsafe.Slice((*T1)(unsafe.Pointer(x.ptr)), x.len)
                output := make([]T2, len(input))
                for i, v := range input {
                    output[i] = f(v)
                }
                return output
            }
        }
        // only handle non-primitive type T
        func cnt_list_mapper[T any](f func(s *T, cnt *uint)) func(s *[]T, cnt *uint) {
            return func(s *[]T, cnt *uint) {
                for _, v := range *s {
                    f(&v, cnt)
                }
                *cnt += uint(len(*s)) * size_of[C.ListRef]()
            }
        }
        // only handle primitive type T
        func cnt_list_mapper_primitive[T any](f func(s *T, cnt *uint)) func(s *[]T, cnt *uint) {
            return func(s *[]T, cnt *uint) {}
        }
        // only handle non-primitive type T
        func ref_list_mapper[T, R any](f func(s *T, buffer *[]byte) R) func(s *[]T, buffer *[]byte) C.ListRef {
            return func(s *[]T, buffer *[]byte) C.ListRef {
                if len(*buffer) == 0 {
                    return C.ListRef{
                        ptr: unsafe.Pointer(nil),
                        len: C.uintptr_t(len(*s)),
                    }
                }
                ret := C.ListRef{
                    ptr: unsafe.Pointer(&(*buffer)[0]),
                    len: C.uintptr_t(len(*s)),
                }
                children_bytes := int(size_of[R]()) * len(*s)
                children := (*buffer)[:children_bytes]
                *buffer = (*buffer)[children_bytes:]
                for _, v := range *s {
                    child := f(&v, buffer)
                    len := unsafe.Sizeof(child)
                    copy(children, unsafe.Slice((*byte)(unsafe.Pointer(&child)), len))
                    children = children[len:]
                }
                return ret
            }
        }
        // only handle primitive type T
        func ref_list_mapper_primitive[T, R any](f func(s *T, buffer *[]byte) R) func(s *[]T, buffer *[]byte) C.ListRef {
            return func(s *[]T, buffer *[]byte) C.ListRef {
                if len(*s) == 0 {
                    return C.ListRef{
                        ptr: unsafe.Pointer(nil),
                        len: C.uintptr_t(0),
                    }
                }
                return C.ListRef{
                    ptr: unsafe.Pointer(&(*s)[0]),
                    len: C.uintptr_t(len(*s)),
                }
            }
        }
        func size_of[T any]() uint {
            var t T
            return uint(unsafe.Sizeof(t))
        }
        func cvt_ref[R, CR any](cnt_f func(s *R, cnt *uint), ref_f func(p *R, buffer *[]byte) CR) func(p *R) (CR, []byte) {
            return func(p *R) (CR, []byte) {
                var cnt uint
                cnt_f(p, &cnt)
                buffer := make([]byte, cnt)
                return ref_f(p, &buffer), buffer
            }
        }

        func newC_uint8_t(n C.uint8_t) uint8    { return uint8(n) }
        func newC_uint16_t(n C.uint16_t) uint16 { return uint16(n) }
        func newC_uint32_t(n C.uint32_t) uint32 { return uint32(n) }
        func newC_uint64_t(n C.uint64_t) uint64 { return uint64(n) }
        func newC_int8_t(n C.int8_t) int8       { return int8(n) }
        func newC_int16_t(n C.int16_t) int16    { return int16(n) }
        func newC_int32_t(n C.int32_t) int32    { return int32(n) }
        func newC_int64_t(n C.int64_t) int64    { return int64(n) }
        func newC_bool(n C.bool) bool           { return bool(n) }
        func newC_uintptr_t(n C.uintptr_t) uint { return uint(n) }
        func newC_intptr_t(n C.intptr_t) int    { return int(n) }
        func newC_float(n C.float) float32      { return float32(n) }
        func newC_double(n C.double) float64    { return float64(n) }

        func cntC_uint8_t(s *uint8, cnt *uint)   {}
        func cntC_uint16_t(s *uint16, cnt *uint) {}
        func cntC_uint32_t(s *uint32, cnt *uint) {}
        func cntC_uint64_t(s *uint64, cnt *uint) {}
        func cntC_int8_t(s *int8, cnt *uint)     {}
        func cntC_int16_t(s *int16, cnt *uint)   {}
        func cntC_int32_t(s *int32, cnt *uint)   {}
        func cntC_int64_t(s *int64, cnt *uint)   {}
        func cntC_bool(s *bool, cnt *uint)       {}
        func cntC_uintptr_t(s *uint, cnt *uint)  {}
        func cntC_intptr_t(s *int, cnt *uint)    {}
        func cntC_float(s *float32, cnt *uint)   {}
        func cntC_double(s *float64, cnt *uint)  {}

        func refC_uint8_t(p *uint8, buffer *[]byte) C.uint8_t    { return C.uint8_t(*p) }
        func refC_uint16_t(p *uint16, buffer *[]byte) C.uint16_t { return C.uint16_t(*p) }
        func refC_uint32_t(p *uint32, buffer *[]byte) C.uint32_t { return C.uint32_t(*p) }
        func refC_uint64_t(p *uint64, buffer *[]byte) C.uint64_t { return C.uint64_t(*p) }
        func refC_int8_t(p *int8, buffer *[]byte) C.int8_t       { return C.int8_t(*p) }
        func refC_int16_t(p *int16, buffer *[]byte) C.int16_t    { return C.int16_t(*p) }
        func refC_int32_t(p *int32, buffer *[]byte) C.int32_t    { return C.int32_t(*p) }
        func refC_int64_t(p *int64, buffer *[]byte) C.int64_t    { return C.int64_t(*p) }
        func refC_bool(p *bool, buffer *[]byte) C.bool           { return C.bool(*p) }
        func refC_uintptr_t(p *uint, buffer *[]byte) C.uintptr_t { return C.uintptr_t(*p) }
        func refC_intptr_t(p *int, buffer *[]byte) C.intptr_t    { return C.intptr_t(*p) }
        func refC_float(p *float32, buffer *[]byte) C.float      { return C.float(*p) }
        func refC_double(p *float64, buffer *[]byte) C.double    { return C.double(*p) }
        "#;
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
                    let struct_name = s.ident.to_string();
                    out.push_str(&format!("type {} struct {{\n", struct_name));
                    for field in s.fields.iter() {
                        let field_name = field
                            .ident
                            .as_ref()
                            .ok_or_else(|| serr!("only named fields are supported"))?
                            .to_string();
                        let field_type = ParamType::try_from(&field.ty)?;
                        out.push_str(&format!("    {} {}\n", field_name, field_type.to_go()));
                    }
                    out.push_str("}\n");

                    // newStruct
                    out.push_str(&format!(
                        "func new{struct_name}(p C.{struct_name}Ref) {struct_name}{{\nreturn {struct_name}{{\n"
                    ));
                    for field in s.fields.iter() {
                        let field_name = field.ident.as_ref().unwrap().to_string();
                        let field_type = ParamType::try_from(&field.ty)?;
                        out.push_str(&format!(
                            "{field_name}: {}(p.{field_name}),\n",
                            field_type.c_to_go_field_converter()
                        ));
                    }
                    out.push_str("}\n}\n");

                    let level = *levels.get(&s.ident).unwrap();

                    // cntStruct
                    out.push_str(&format!(
                        "func cnt{struct_name}(s *{struct_name}, cnt *uint) {{\n"
                    ));
                    if level == 2 {
                        for field in s.fields.iter() {
                            let field_name = field.ident.as_ref().unwrap().to_string();
                            let field_type = ParamType::try_from(&field.ty)?;
                            let (counter_f, level) = field_type.go_to_c_field_counter(levels);
                            if level == 2 {
                                out.push_str(&format!("{counter_f}(&s.{field_name}, cnt)\n"));
                            }
                        }
                    }
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

    pub fn convert_trait(&self) -> Result<Vec<TraitRepr>> {
        let out: Vec<TraitRepr> = self
            .file
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Trait(t) => Some(t),
                _ => None,
            })
            .map(|trat| trat.try_into())
            .collect::<Result<Vec<TraitRepr>>>()?;
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
            match seg.ident.to_string().as_str() {
                "Vec" => {
                    let inside = match &seg.arguments {
                        syn::PathArguments::AngleBracketed(ga) => match ga.args.last().unwrap() {
                            syn::GenericArgument::Type(ty) => ty,
                            _ => panic!("list generic must be a type"),
                        },
                        _ => panic!("list type must have angle bracketed arguments"),
                    };
                    Ok(Node::List(Box::new(type_to_node(inside)?)))
                }
                "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" | "bool" | "char" => {
                    Ok(Node::Primitive)
                }
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
}

pub struct TraitRepr {
    name: Ident,
    fns: Vec<FnRepr>,
}

impl TryFrom<&ItemTrait> for TraitRepr {
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
                    _ => sbail!("only path type returns are supported"),
                },
            };
            if is_async && ret.is_none() {
                sbail!("async function must have a return value")
            }

            // on async mode, parse attributes to check it's drop safe setting.
            let mut drop_safe_ret_params = false;
            let mut ret_send = false;

            let mut safe = true;
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
                    safe = false;
                }
                if (drop_safe || drop_safe_ret_params) && has_reference {
                    sbail!("drop_safe function cannot have reference parameters")
                }
            }

            fns.push(FnRepr {
                name: fn_name,
                is_async,
                params,
                ret,
                safe,
                drop_safe_ret_params,
                ret_send,
                ret_static: !has_reference,
            });
        }
        Ok(TraitRepr {
            name: trait_name,
            fns,
        })
    }
}

pub struct FnRepr {
    name: Ident,
    is_async: bool,
    params: Vec<Param>,
    ret: Option<ParamType>,
    safe: bool,
    drop_safe_ret_params: bool,
    ret_send: bool,
    ret_static: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropSafe {
    ThreadLocal,
    Global,
    None,
}

pub struct Param {
    name: Ident,
    ty: ParamType,
}

impl Param {
    pub fn ty(&self) -> &ParamType {
        &self.ty
    }
}

pub struct ParamType {
    inner: ParamTypeInner,
    is_reference: bool,
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
        let param_type_inner = match seg.ident.to_string().as_str() {
            "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "bool" | "char"
            | "f32" => {
                if !seg.arguments.is_none() {
                    sbail!("primitive types with arguments are not supported")
                }
                ParamTypeInner::Primitive(seg.ident.clone())
            }
            "Vec" => ParamTypeInner::List(ty.clone()),
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
    fn to_c(&self, with_struct: bool) -> String {
        let struct_ = if with_struct { "struct " } else { "" };
        match &self.inner {
            ParamTypeInner::Primitive(name) => match name.to_string().as_str() {
                "u8" => "uint8_t",
                "u16" => "uint16_t",
                "u32" => "uint32_t",
                "u64" => "uint64_t",
                "i8" => "int8_t",
                "i16" => "int16_t",
                "i32" => "int32_t",
                "i64" => "int64_t",
                "bool" => "bool",
                "char" => "uint32_t",
                "usize" => "uintptr_t",
                "isize" => "intptr_t",
                "f32" => "float",
                "f64" => "double",
                _ => panic!("unreconigzed rust primitive type {name}"),
            }
            .to_string(),
            ParamTypeInner::Custom(c) => format!("{struct_}{c}Ref"),
            ParamTypeInner::List(_) => format!("{struct_}ListRef"),
        }
    }

    fn to_go(&self) -> String {
        match &self.inner {
            ParamTypeInner::Primitive(name) => match name.to_string().as_str() {
                "u8" => "uint8",
                "u16" => "uint16",
                "u32" => "uint32",
                "u64" => "uint64",
                "i8" => "int8",
                "i16" => "int16",
                "i32" => "int32",
                "i64" => "int64",
                "bool" => "bool",
                "char" => "rune",
                "usize" => "uint",
                "isize" => "int",
                "f32" => "float32",
                "f64" => "float64",
                _ => panic!("unreconigzed rust primitive type {name}"),
            }
            .to_string(),
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
    fn c_to_go_field_converter(&self) -> String {
        match &self.inner {
            ParamTypeInner::Primitive(name) => match name.to_string().as_str() {
                "u8" => "newC_uint8_t",
                "u16" => "newC_uint16_t",
                "u32" => "newC_uint32_t",
                "u64" => "newC_uint64_t",
                "i8" => "newC_int8_t",
                "i16" => "newC_int16_t",
                "i32" => "newC_int32_t",
                "i64" => "newC_int64_t",
                "bool" => "newC_bool",
                "usize" => "newC_uintptr_t",
                "isize" => "newC_intptr_t",
                "f32" => "newC_float",
                "f64" => "newC_double",
                _ => panic!("unreconigzed rust primitive type {name}"),
            }
            .to_string(),
            ParamTypeInner::Custom(c) => format!("new{}", c.to_string().as_str()),
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
                    "new_list_mapper({})",
                    ParamType::try_from(inside)
                        .expect("unable to convert list type")
                        .c_to_go_field_converter()
                )
            }
        }
    }

    fn go_to_c_field_counter(&self, mapping: &HashMap<Ident, u8>) -> (String, u8) {
        match &self.inner {
            ParamTypeInner::Primitive(name) => (
                match name.to_string().as_str() {
                    "u8" => "cntC_uint8_t",
                    "u16" => "cntC_uint16_t",
                    "u32" => "cntC_uint32_t",
                    "u64" => "cntC_uint64_t",
                    "i8" => "cntC_int8_t",
                    "i16" => "cntC_int16_t",
                    "i32" => "cntC_int32_t",
                    "i64" => "cntC_int64_t",
                    "bool" => "cntC_bool",
                    "usize" => "cntC_uintptr_t",
                    "isize" => "cntC_intptr_t",
                    "f32" => "cntC_float",
                    "f64" => "cntC_double",
                    _ => panic!("unreconigzed rust primitive type {name}"),
                }
                .to_string(),
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
    fn go_to_c_field_converter(&self, mapping: &HashMap<Ident, u8>) -> (String, u8) {
        match &self.inner {
            ParamTypeInner::Primitive(name) => (
                match name.to_string().as_str() {
                    "u8" => "refC_uint8_t",
                    "u16" => "refC_uint16_t",
                    "u32" => "refC_uint32_t",
                    "u64" => "refC_uint64_t",
                    "i8" => "refC_int8_t",
                    "i16" => "refC_int16_t",
                    "i32" => "refC_int32_t",
                    "i64" => "refC_int64_t",
                    "bool" => "refC_bool",
                    "usize" => "refC_uintptr_t",
                    "isize" => "refC_intptr_t",
                    "f32" => "refC_float",
                    "f64" => "refC_double",
                    _ => panic!("unreconigzed rust primitive type {name}"),
                }
                .to_string(),
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

    fn to_rust_ref(&self) -> Ident {
        match &self.inner {
            ParamTypeInner::Primitive(name) => name.clone(),
            ParamTypeInner::Custom(name) => format_ident!("{}Ref", name),
            ParamTypeInner::List(_) => format_ident!("ListRef"),
        }
    }
}

impl TraitRepr {
    pub fn fns(&self) -> &[FnRepr] {
        &self.fns
    }

    // Generate c callbacks used in golang import "C".
    pub fn generate_c_callbacks(&self) -> String {
        let name = self.name.to_string();
        self.fns.iter().map(|f| f.to_c_callback(&name)).collect()
    }

    // Generate golang exports.
    pub fn generate_go_exports(&self, levels: &HashMap<Ident, u8>) -> String {
        let name = self.name.to_string();
        self.fns
            .iter()
            .map(|f| f.to_go_export(&name, levels))
            .collect()
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
    pub fn generate_rs(&self, binding_path: Option<&Path>) -> Result<TokenStream> {
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
        Ok(quote! {
            pub struct #impl_struct_name;
            impl #trait_name for #impl_struct_name {
                #(#fn_trait_impls)*
            }
            impl #impl_struct_name {
                #(#fn_callbacks)*
            }
        })
    }
}

impl FnRepr {
    pub fn name(&self) -> &Ident {
        &self.name
    }

    pub fn is_async(&self) -> bool {
        self.is_async
    }

    pub fn drop_safe_ret_params(&self) -> bool {
        self.drop_safe_ret_params
    }

    pub fn safe(&self) -> bool {
        self.safe
    }

    pub fn params(&self) -> &[Param] {
        &self.params
    }

    pub fn ret(&self) -> Option<&ParamType> {
        self.ret.as_ref()
    }

    pub fn ret_send(&self) -> bool {
        self.ret_send
    }

    pub fn ret_static(&self) -> bool {
        self.ret_static
    }

    fn to_c_callback(&self, trait_name: &str) -> String {
        let Some(ret) = &self.ret else {
            return String::new();
        };

        let fn_name = format!("{}_{}", trait_name, self.name);
        let c_resp_type = ret.to_c(true);

        match self.is_async {
            true => format!(
                r#"
// hack from: https://stackoverflow.com/a/69904977
__attribute__((weak))
inline void {fn_name}_cb(const void *f_ptr, {c_resp_type} resp, const void *slot) {{
((void (*)({c_resp_type}, const void*))f_ptr)(resp, slot);
}}
"#,
            ),
            false => format!(
                r#"
// hack from: https://stackoverflow.com/a/69904977
__attribute__((weak))
inline void {fn_name}_cb(const void *f_ptr, {c_resp_type} resp, const void *slot) {{
((void (*)({c_resp_type}, const void*))f_ptr)(resp, slot);
}}
"#,
            ),
        }
    }

    fn to_go_export(&self, trait_name: &str, levels: &HashMap<Ident, u8>) -> String {
        let mut out = String::new();
        let fn_name = format!("C{}_{}", trait_name, self.name);
        let callback = format!("{}_{}_cb", trait_name, self.name);
        out.push_str(&format!("//export {fn_name}\nfunc {fn_name}("));
        self.params
            .iter()
            .for_each(|p| out.push_str(&format!("{} C.{}, ", p.name, p.ty.to_c(false))));

        match (self.is_async, &self.ret) {
            (true, None) => panic!("async function must have a return value"),
            (false, None) => {
                // //export CDemoCall_demo_oneway
                // func CDemoCall_demo_oneway(req C.DemoUserRef) {
                //     DemoCallImpl.demo_oneway(newDemoUser(req))
                // }
                out.push_str(") {\n");
                out.push_str(&format!(
                    "    {trait_name}Impl.{fn_name}({params})\n",
                    fn_name = self.name,
                    params = self
                        .params
                        .iter()
                        .map(|p| format!("{}({})", p.ty.c_to_go_field_converter(), p.name))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                out.push_str("}\n");
            }
            (false, Some(ret)) => {
                // //export CDemoCall_demo_check
                // func CDemoCall_demo_check(req C.DemoComplicatedRequestRef, slot *C.void, cb *C.void) {
                //     resp := DemoCallImpl.demo_check(newDemoComplicatedRequest(req))
                //     resp_ref, buffer := cvt_ref(cntDemoResponse, refDemoResponse)(&resp)
                //     C.DemoCall_demo_check_cb(unsafe.Pointer(cb), resp_ref, unsafe.Pointer(slot))
                //     runtime.KeepAlive(resp)
                //     runtime.KeepAlive(buffer)
                // }
                out.push_str("slot *C.void, cb *C.void) {\n");
                out.push_str(&format!(
                    "resp := {trait_name}Impl.{fn_name}({params})\n",
                    fn_name = self.name,
                    params = self
                        .params
                        .iter()
                        .map(|p| format!("{}({})", p.ty.c_to_go_field_converter(), p.name))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                let (g2c_cnt, g2c_cvt) = (
                    ret.go_to_c_field_counter(levels).0,
                    ret.go_to_c_field_converter(levels).0,
                );
                out.push_str(&format!(
                    "resp_ref, buffer := cvt_ref({g2c_cnt}, {g2c_cvt})(&resp)\n"
                ));
                out.push_str(&format!(
                    "C.{callback}(unsafe.Pointer(cb), resp_ref, unsafe.Pointer(slot))\n",
                ));
                out.push_str("runtime.KeepAlive(resp)\nruntime.KeepAlive(buffer)\n");
                out.push_str("}\n");
            }
            (true, Some(ret)) => {
                // //export CDemoCall_demo_check_async
                // func CDemoCall_demo_check_async(req C.DemoComplicatedRequestRef, slot *C.void, cb *C.void) {
                //     go func() {
                //         resp := DemoCallImpl.demo_check_async(newDemoComplicatedRequest(req))
                //         resp_ref, buffer := cvt_ref(cntDemoResponse, refDemoResponse)(&resp)
                //         C.DemoCall_demo_check_async_cb(unsafe.Pointer(cb), resp_ref, unsafe.Pointer(slot))
                //         runtime.KeepAlive(resp)
                //         runtime.KeepAlive(buffer)
                //     }()
                // }
                out.push_str("slot *C.void, cb *C.void) {\n");
                out.push_str("    go func() {\n");
                out.push_str(&format!(
                    "resp := {trait_name}Impl.{fn_name}({params})\n",
                    fn_name = self.name,
                    params = self
                        .params
                        .iter()
                        .map(|p| format!("{}({})", p.ty.c_to_go_field_converter(), p.name))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                let (g2c_cnt, g2c_cvt) = (
                    ret.go_to_c_field_counter(levels).0,
                    ret.go_to_c_field_converter(levels).0,
                );
                out.push_str(&format!(
                    "resp_ref, buffer := cvt_ref({g2c_cnt}, {g2c_cvt})(&resp)\n"
                ));
                out.push_str(&format!(
                    "C.{callback}(unsafe.Pointer(cb), resp_ref, unsafe.Pointer(slot))\n",
                ));
                out.push_str("runtime.KeepAlive(resp)\nruntime.KeepAlive(buffer)\n");
                out.push_str("}()\n}\n");
            }
        }
        out
    }

    fn to_go_interface_method(&self) -> String {
        // demo_oneway(req DemoUser)
        // demo_check(req DemoComplicatedRequest) DemoResponse
        format!(
            "{}({}) {}",
            self.name,
            self.params
                .iter()
                .map(|p| format!("{} {}", p.name, p.ty.to_go()))
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
        let unsafe_marker = (!self.safe).then(syn::token::Unsafe::default);
        out.extend(quote! {
            #unsafe_marker fn #func_name(#(#func_param_names: #func_param_types)*)
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
                // fn demo_check(r: user::DemoRequest) {
                //     let (_buf, r) = ::rust2go::ToRef::calc_ref(&r);
                //     unsafe {binding::CDemoCall_demo_check(::std::mem::transmute(r))}
                // }
                out.extend(quote! {
                    {
                        #(
                            let (_buf, #func_param_names) = ::rust2go::ToRef::calc_ref(#ref_marks #func_param_names);
                        )*
                        unsafe {#path_prefix #c_func_name(#(::std::mem::transmute(#func_param_names)),*)}
                    }
                });
            }
            (false, Some(ret)) => {
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
                        unsafe { #path_prefix #c_func_name(#(::std::mem::transmute(#func_param_names)),*, &slot as *const _ as *const () as *mut _, Self::#callback_name as *const () as *mut _) };
                        slot.take().unwrap()
                    }
                });
            }
            (true, Some(ret)) => {
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
                            unsafe {
                                #path_prefix #c_func_name(
                                    #(::std::mem::transmute(r_ref.#tuple_ids),)*
                                    slot as *const _ as *mut _,
                                    cb as *const _ as *mut _,
                                )
                            };
                        },
                        #((#func_param_names,))*,
                        Self::#callback_name as *const (),
                    )
                    }
                });
            }
        }
        Ok(out)
    }

    fn to_rs_callback(&self, path_prefix: &TokenStream) -> Result<TokenStream> {
        let fn_name = format_ident!("{}_cb", self.name);

        match (self.is_async, &self.ret) {
            (true, None) => sbail!("async function must have a return value"),
            (false, None) => {
                // There's no need to generate callback for sync function without callback.
                Ok(TokenStream::default())
            }
            (false, Some(ret)) => {
                // #[no_mangle]
                // unsafe extern "C" fn demo_check_cb(resp: binding::DemoResponseRef, slot: *const ()) {
                //     *(slot as *mut Option<DemoResponse>) = Some(::rust2go::FromRef::from_ref(::std::mem::transmute(&resp)));
                // }
                let resp_ref_ty = ret.to_rust_ref();
                Ok(quote! {
                    #[no_mangle]
                    unsafe extern "C" fn #fn_name(resp: #path_prefix #resp_ref_ty, slot: *const ()) {
                        *(slot as *mut Option<#ret>) = Some(::rust2go::FromRef::from_ref(::std::mem::transmute(&resp)));
                    }
                })
            }
            (true, Some(ret)) => {
                // #[no_mangle]
                // unsafe extern "C" fn demo_check_async_cb(
                //     resp: binding::DemoResponseRef,
                //     slot: *const (),
                // ) {
                //     ::rust2go::SlotWriter::<DemoResponse>::from_ptr(slot).write(::rust2go::FromRef::from_ref(::std::mem::transmute(&resp)));
                // }
                let resp_ref_ty = ret.to_rust_ref();
                let func_param_types = self.params.iter().map(|p| &p.ty);
                Ok(quote! {
                    #[no_mangle]
                    unsafe extern "C" fn #fn_name(resp: #path_prefix #resp_ref_ty, slot: *const ()) {
                        ::rust2go::SlotWriter::<#ret, ((#(#func_param_types,)*), Vec<u8>)>::from_ptr(slot).write(::rust2go::FromRef::from_ref(::std::mem::transmute(&resp)));
                    }
                })
            }
        }
    }
}

fn type_to_segment(ty: &Type) -> Result<&PathSegment> {
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
        let traits = raw_file.convert_trait().unwrap();
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
}
