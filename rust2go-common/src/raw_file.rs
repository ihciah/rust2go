use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use std::collections::HashMap;
use syn::{
    Error, File, FnArg, Ident, Item, ItemTrait, Pat, Path, PathSegment, Result, ReturnType, Token,
    TraitItem, Type,
};

macro_rules! serr {
    ($msg:expr) => {
        Error::new(Span::call_site(), $msg)
    };
}

macro_rules! sbail {
    ($msg:expr) => {
        return Err(Error::new(Span::call_site(), $msg))
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
    pub fn convert_to_ref(&self) -> Result<(HashMap<Ident, Ident>, TokenStream)> {
        let mut name_mapping = HashMap::new();

        // Add these to generated code to make golang have C structs of waker and string.
        let mut out = quote! {
            #[repr(C)]
            pub struct WakerRef {
                pub ptr: *const (),
                pub vtable: *const (),
            }
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
            Ident::new("Waker", Span::call_site()),
            Ident::new("WakerRef", Span::call_site()),
        );
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
            fns.push(FnRepr {
                name: fn_name,
                is_async,
                params,
                ret,
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
}

pub struct Param {
    name: Ident,
    ty: ParamType,
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

    fn to_rust_ref(&self) -> Ident {
        match &self.inner {
            ParamTypeInner::Primitive(name) => name.clone(),
            ParamTypeInner::Custom(name) => format_ident!("{}Ref", name),
            ParamTypeInner::List(_) => format_ident!("ListRef"),
        }
    }
}

impl TraitRepr {
    // Generate c callbacks used in golang import "C".
    pub fn generate_c_callbacks(&self) -> String {
        let name = self.name.to_string();
        self.fns.iter().map(|f| f.to_c_callback(&name)).collect()
    }

    // Generate golang exports.
    pub fn generate_go_exports(&self) -> String {
        let name = self.name.to_string();
        self.fns.iter().map(|f| f.to_go_export(&name)).collect()
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
inline void {fn_name}_cb(const void *f_ptr, struct WakerRef waker, {c_resp_type} resp, const void *slot) {{
((void (*)(struct WakerRef, {c_resp_type}, const void*))f_ptr)(waker, resp, slot);
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

    fn to_go_export(&self, trait_name: &str) -> String {
        let mut out = String::new();
        let fn_name = format!("C{}_{}", trait_name, self.name);
        let callback = format!("{}_{}_cb", trait_name, self.name);
        out.push_str(&format!("//export {fn_name}\nfunc {fn_name}"));

        match (self.is_async, &self.ret) {
            (true, None) => panic!("async function must have a return value"),
            (false, None) => {
                // //export CDemoCheck
                // func CDemoCheck(_ C.DemoRequestRef) {
                //     // user logic
                // }
                out.push('(');
                self.params
                    .iter()
                    .for_each(|p| out.push_str(&format!("_ C.{}, ", p.ty.to_c(false))));
                out.push_str(") {\n    // user logic\n}\n");
            }
            (false, Some(ret)) => {
                // //export CDemoCheck
                // func CDemoCheck(_ C.DemoRequestRef, slot *C.void, cb *C.void) {
                //     // user logic
                //     resp := C.DemoResponseRef {}
                //     C.demo_check_cb(unsafe.Pointer(cb), resp, unsafe.Pointer(slot))
                // }
                out.push('(');
                self.params
                    .iter()
                    .for_each(|p| out.push_str(&format!("_ C.{}, ", p.ty.to_c(false))));

                out.push_str("slot *C.void, cb *C.void) {\n    // user logic\n");
                out.push_str(&format!("    resp := C.{}{{}}\n", ret.to_c(false)));
                out.push_str(&format!(
                    "    C.{callback}(unsafe.Pointer(cb), resp, unsafe.Pointer(slot))\n"
                ));
                out.push_str("}\n");
            }
            (true, Some(ret)) => {
                // //export CDemoCheckAsync
                // func CDemoCheckAsync(w C.WakerRef, r C.DemoRequestRef, slot *C.void, cb *C.void) {
                //     go func() {
                //       // user logic
                //       resp := C.DemoResponseRef {}
                //       C.demo_check_async_cb(unsafe.Pointer(cb), w, resp, unsafe.Pointer(slot))
                //     }()
                // }
                out.push_str("(w C.WakerRef, ");
                self.params
                    .iter()
                    .for_each(|p| out.push_str(&format!("_ C.{}, ", p.ty.to_c(false))));

                out.push_str("slot *C.void, cb *C.void) {\n");
                out.push_str("    go func() {\n");
                out.push_str("        // user logic\n");
                out.push_str(&format!("    resp := C.{}{{}}\n", ret.to_c(false)));
                out.push_str(&format!(
                    "        C.{callback}(unsafe.Pointer(cb), w, resp, unsafe.Pointer(slot))\n"
                ));
                out.push_str("    }()\n");
                out.push_str("}\n");
            }
        }
        out
    }

    fn to_rs_impl(&self, trait_name: &Ident, path_prefix: &TokenStream) -> Result<TokenStream> {
        let mut out = TokenStream::default();

        let func_name = &self.name;
        let func_param_names: Vec<_> = self.params.iter().map(|p| &p.name).collect();
        let func_param_types: Vec<_> = self.params.iter().map(|p| &p.ty).collect();
        out.extend(quote! {
            fn #func_name(#(#func_param_names: #func_param_types)*)
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

                let callback_name = format_ident!("{func_name}_cb");
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
                //         |waker: std::task::Waker, r: (user::DemoRequest,), slot: *const (), cb: *const ()| {
                //             let (_, waker_ref) = ::rust2go::ToRef::calc_ref(&waker);
                //             std::mem::forget(waker);
                //
                //             let size = ::rust2go::ToRef::calc_size(&r);
                //             let mut buffer = ::std::vec::Vec::<u8>::with_capacity(size);
                //             let mut writer = unsafe { ::rust2go::Writer::new(buffer.as_ptr() as _) };
                //             let r_ref = ::rust2go::ToRef::to_ref(&r, &mut writer);
                //             unsafe { buffer.set_len(size) };
                //             unsafe {
                //                 binding::CDemoCall_demo_check_async(
                //                     ::std::mem::transmute(waker_ref),
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
                out.extend(quote! {
                    -> impl ::std::future::Future<Output = #ret> {
                    ::rust2go::ResponseFuture::Init(
                        |waker: ::std::task::Waker, r: (#(#func_param_types,)*), slot: *const (), cb: *const ()| {
                            let (_, waker_ref) = ::rust2go::ToRef::calc_ref(&waker);
                            ::std::mem::forget(waker);

                            let size = ::rust2go::ToRef::calc_size(&r);
                            let mut buffer = ::std::vec::Vec::<u8>::with_capacity(size);
                            let mut writer = unsafe { ::rust2go::Writer::new(buffer.as_ptr() as _) };
                            let r_ref = ::rust2go::ToRef::to_ref(&r, &mut writer);
                            unsafe { buffer.set_len(size) };
                            unsafe {
                                #path_prefix #c_func_name(
                                    ::std::mem::transmute(waker_ref),
                                    #(::std::mem::transmute(r_ref.#tuple_ids),)*
                                    slot as *const _ as *mut _,
                                    cb as *const _ as *mut _,
                                )
                            };
                        },
                        #((#func_param_names,))*,
                        Self::demo_check_async_cb as *const (),
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
                //     waker: binding::WakerRef,
                //     resp: binding::DemoResponseRef,
                //     slot: *const (),
                // ) {
                //     ::rust2go::SlotWriter::<DemoResponse>::from_ptr(slot).write(::rust2go::FromRef::from_ref(::std::mem::transmute(&resp)));
                //     <::std::task::Waker as ::rust2go::FromRef>::from_ref(::std::mem::transmute(&waker)).wake();
                // }
                let resp_ref_ty = ret.to_rust_ref();
                Ok(quote! {
                    #[no_mangle]
                    unsafe extern "C" fn #fn_name(waker: #path_prefix WakerRef, resp: #path_prefix #resp_ref_ty, slot: *const ()) {
                        ::rust2go::SlotWriter::<#ret>::from_ptr(slot).write(::rust2go::FromRef::from_ref(::std::mem::transmute(&resp)));
                        <::std::task::Waker as ::rust2go::FromRef>::from_ref(::std::mem::transmute(&waker)).wake();
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
