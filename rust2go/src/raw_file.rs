use quote::ToTokens;
use std::{borrow::Cow, collections::HashMap};

pub struct RawRsFile {
    file: syn::File,
}

impl RawRsFile {
    pub fn new<S: AsRef<str>>(src: S) -> Self {
        let src = src.as_ref();
        let syntax = syn::parse_file(src).expect("Unable to parse file");
        RawRsFile { file: syntax }
    }

    pub fn convert_to_ref(&self) -> anyhow::Result<(HashMap<String, String>, String)> {
        let mut name_mapping = HashMap::new();
        let mut result = String::new();

        name_mapping.insert("Waker".to_string(), "WakerRef".to_string());
        name_mapping.insert("String".to_string(), "StringRef".to_string());
        // Add these to generated code to make golang have C structs of waker and string.
        result.push_str(
            r#"
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
"#,
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
                syn::Item::Struct(s) => {
                    let struct_name = s.ident.to_string();
                    let struct_name_ref = format!("{}Ref", struct_name);
                    name_mapping.insert(struct_name, struct_name_ref.clone());
                    result.push_str(&format!("#[repr(C)]\npub struct {struct_name_ref} {{\n"));
                    for field in s.fields.iter() {
                        let field_name = field
                            .ident
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!("only named fields are supported"))?
                            .to_string();
                        let new_field_type = type_to_ref_ident(&field.ty).unwrap();
                        result.push_str(&format!("pub {field_name}: {new_field_type},\n"));
                    }
                    result.push('}');
                }
                _ => continue,
            }
        }

        Ok((name_mapping, result))
    }

    pub fn convert_trait(&self) -> anyhow::Result<Vec<TraitRepr>> {
        let mut out = Vec::new();
        for item in self.file.items.iter() {
            let tra = match item {
                syn::Item::Trait(t) => t,
                _ => continue,
            };
            let name = tra.ident.to_string();
            let mut fns = Vec::new();
            for item in tra.items.iter() {
                let fn_item = match item {
                    syn::TraitItem::Fn(m) => m,
                    _ => anyhow::bail!("only fn items are supported"),
                };
                let fn_name = fn_item.sig.ident.to_string();
                let mut is_async = fn_item.sig.asyncness.is_some();
                let mut params = Vec::new();
                for param in fn_item.sig.inputs.iter() {
                    let param = match param {
                        syn::FnArg::Typed(t) => t,
                        _ => anyhow::bail!("only typed fn args are supported"),
                    };
                    let param_name = match param.pat.as_ref() {
                        syn::Pat::Ident(i) => i.ident.to_string(),
                        _ => anyhow::bail!("only ident fn args are supported"),
                    };
                    let mut ty = param.ty.as_ref();
                    let mut is_reference = false;
                    if let syn::Type::Reference(r) = param.ty.as_ref() {
                        is_reference = true;
                        ty = &r.elem;
                    }

                    let param_type = match ty {
                        syn::Type::Path(p) => p,
                        _ => anyhow::bail!("only path type params are supported, ty: {ty:?}"),
                    };
                    let param_type_str = param_type
                        .path
                        .get_ident()
                        .ok_or_else(|| {
                            anyhow::anyhow!("only single ident path types are supported")
                        })
                        .unwrap()
                        .to_string();
                    params.push(Param {
                        name: param_name,
                        ty: param_type_str,
                        is_reference,
                    });
                }
                let ret = match &fn_item.sig.output {
                    syn::ReturnType::Default => None,
                    syn::ReturnType::Type(_, t) => match t.as_ref() {
                        syn::Type::Path(_) => Some(type_to_string(t)?),
                        // Check if it's a future.
                        syn::Type::ImplTrait(i) => {
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
                                                let ret = Some(type_to_string(&t.ty).ok()?);
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
                                .ok_or_else(|| {
                                    anyhow::anyhow!("only future types are supported")
                                })?;
                            Some(t_str)
                        }
                        _ => anyhow::bail!("only path type returns are supported"),
                    },
                };
                if is_async && ret.is_none() {
                    anyhow::bail!("async function must have a return value")
                }
                fns.push(FnRepr {
                    name: fn_name,
                    is_async,
                    params,
                    ret,
                });
            }
            out.push(TraitRepr { name, fns });
        }
        Ok(out)
    }
}

#[derive(Debug)]
pub struct TraitRepr {
    name: String,
    fns: Vec<FnRepr>,
}

#[derive(Debug)]
pub struct FnRepr {
    name: String,
    is_async: bool,
    params: Vec<Param>,
    ret: Option<String>,
}

#[derive(Debug)]
pub struct Param {
    name: String,
    ty: String,
    is_reference: bool,
}

impl Param {
    pub fn c_param(&self, mapping: &HashMap<String, String>) -> String {
        let param_type = match mapping.get(&self.ty) {
            Some(ref_struct) => format!("C.{ref_struct}"),
            None => format!("C.{}", (rust_primitive_to_c(&self.ty))),
        };
        format!("_ {param_type}, ")
    }

    pub fn rs_param(&self) -> String {
        let mut out = String::new();
        out.push_str(&self.name);
        out.push_str(": ");
        if self.is_reference {
            out.push('&');
        }
        out.push_str(&self.ty);
        out.push_str(", ");
        out
    }

    pub fn rs_type(&self) -> String {
        let mut out = String::new();
        if self.is_reference {
            out.push('&');
        }
        out.push_str(&self.ty);
        out.push_str(", ");
        out
    }

    pub fn rs_ref_calc(&self) -> String {
        let mut out = String::new();
        let ref_mark = if !self.is_reference { "&" } else { "" };
        out.push_str(&format!(
            "let (_buf, {}) = ::rust2go::ToRef::calc_ref({ref_mark}{});",
            self.name, self.name
        ));

        out
    }
}

impl TraitRepr {
    pub fn generate_c_callbacks(&self, mapping: &HashMap<String, String>) -> String {
        let mut out = String::new();
        for fn_ in self.fns.iter().filter(|f| f.ret.is_some()) {
            let fn_name = format!("{}_{}", self.name, fn_.name);
            let resp_name = fn_.ret.as_ref().unwrap();
            let resp_name = match mapping.get(resp_name) {
                Some(ref_struct) => Cow::Owned(format!("struct {ref_struct}")),
                None => Cow::Borrowed(rust_primitive_to_c(resp_name)),
            };

            match fn_.is_async {
                true => out.push_str(&format!(
                    r#"
// hack from: https://stackoverflow.com/a/69904977
__attribute__((weak))
void {fn_name}_cb(const void *f_ptr, struct WakerRef waker, {resp_name} resp, const void *slot) {{
    ((void (*)(struct WakerRef, {resp_name}, const void*))f_ptr)(waker, resp, slot);
}}
"#,
                )),
                false => out.push_str(&format!(
                    r#"
// hack from: https://stackoverflow.com/a/69904977
__attribute__((weak))
void {fn_name}_cb(const void *f_ptr, {resp_name} resp, const void *slot) {{
    ((void (*)({resp_name}, const void*))f_ptr)(resp, slot);
}}
"#,
                )),
            }
        }
        out
    }

    pub fn generate_go_exports(&self, mapping: &HashMap<String, String>) -> String {
        let mut out = String::new();
        for fn_ in self.fns.iter() {
            let fn_name = format!("C{}_{}", self.name, fn_.name);
            let callback = format!("{}_{}_cb", self.name, fn_.name);
            out.push_str(&format!("//export {fn_name}\nfunc {fn_name}"));
            match (fn_.is_async, &fn_.ret) {
                (true, None) => panic!("async function must have a return value"),
                (false, None) => {
                    // //export CDemoCheck
                    // func CDemoCheck(_ C.DemoRequestRef) {
                    //     // user logic
                    // }
                    out.push('(');
                    fn_.params
                        .iter()
                        .for_each(|p| out.push_str(&p.c_param(mapping)));
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
                    fn_.params
                        .iter()
                        .for_each(|p| out.push_str(&p.c_param(mapping)));

                    out.push_str("slot *C.void, cb *C.void) {\n    // user logic\n");
                    let ret_type = match mapping.get(ret) {
                        Some(ref_struct) => format!("C.{ref_struct}"),
                        None => format!("C.{}", (rust_primitive_to_c(ret))),
                    };
                    out.push_str(&format!("    resp := {ret_type}{{}}\n"));
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
                    fn_.params
                        .iter()
                        .for_each(|p| out.push_str(&p.c_param(mapping)));

                    out.push_str("slot *C.void, cb *C.void) {\n");
                    out.push_str("    go func() {\n");
                    out.push_str("        // user logic\n");
                    let ret_type = match mapping.get(ret) {
                        Some(ref_struct) => format!("C.{ref_struct}"),
                        None => format!("C.{}", (rust_primitive_to_c(ret))),
                    };
                    out.push_str(&format!("        resp := {ret_type}{{}}\n"));
                    out.push_str(&format!(
                        "        C.{callback}(unsafe.Pointer(cb), w, resp, unsafe.Pointer(slot))\n"
                    ));
                    out.push_str("    }()\n");
                    out.push_str("}\n");
                }
            }
        }
        out
    }

    pub fn generate_rs(
        &self,
        mapping: &HashMap<String, String>,
        rs_file_name: Option<&str>,
    ) -> String {
        let mut out = String::new();
        let (fn_trait_impls, fn_callbacks): (Vec<_>, Vec<_>) = self
            .fns
            .iter()
            .map(|f| {
                (
                    f.generate_rs_impl(&self.name),
                    f.generate_rs_callback(mapping),
                )
            })
            .unzip();
        let rs_name = rs_file_name.unwrap_or(crate::DEFAULT_BINDING_NAME);
        out.push_str(&format!(
            "pub mod binding {{ #![allow(warnings)] include!(concat!(env!(\"OUT_DIR\"), \"/{rs_name}\")); }}"
        ));
        out.push_str(&format!("\npub struct {}Impl;\n", self.name));
        out.push_str(&format!("impl {} for {}Impl {{\n", self.name, self.name));
        fn_trait_impls.iter().for_each(|imp| out.push_str(imp));
        out.push_str("}\n");
        out.push_str(&format!("impl {}Impl {{\n", self.name));
        fn_callbacks.iter().for_each(|cb| out.push_str(cb));
        out.push_str("}\n");
        out
    }
}

impl FnRepr {
    fn generate_rs_impl(&self, trait_name: &str) -> String {
        let mut out = String::new();
        out.push_str(&format!("fn {}(", self.name));
        self.params.iter().for_each(|p| out.push_str(&p.rs_param()));
        out.push(')');
        match (self.is_async, &self.ret) {
            (true, None) => panic!("async function must have a return value"),
            (false, None) => {
                // fn demo_check(r: user::DemoRequest) {
                //     let (_buf, r) = ::rust2go::ToRef::calc_ref(&r);
                //     unsafe {binding::CDemoCall_demo_check(::std::mem::transmute(r))}
                // }
                out.push_str(" {\n");
                self.params
                    .iter()
                    .for_each(|p| out.push_str(&p.rs_ref_calc()));
                out.push_str(&format!(
                    "    unsafe {{ binding::C{trait_name}_{}(",
                    self.name
                ));
                self.params
                    .iter()
                    .for_each(|p| out.push_str(&format!("::std::mem::transmute({}),", p.name)));
                out.push_str(")}\n}\n");
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
                out.push_str(&format!(" -> {ret} {{\n"));
                out.push_str("    let mut slot = None;\n");
                self.params
                    .iter()
                    .for_each(|p| out.push_str(&p.rs_ref_calc()));
                out.push_str(&format!(
                    "    unsafe {{ binding::C{trait_name}_{}(",
                    self.name
                ));
                self.params
                    .iter()
                    .for_each(|p| out.push_str(&format!("::std::mem::transmute({}),", p.name)));
                out.push_str(&format!(
                    "&slot as *const _ as *const () as *mut _,
                    Self::{}_cb as *const () as *mut _",
                    self.name
                ));
                out.push_str(")}\n");
                out.push_str("    slot.take().unwrap()\n}\n");
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
                out.push_str(&format!(
                    " -> impl ::std::future::Future<Output = {ret}> {{\n",
                ));
                out.push_str("    ::rust2go::ResponseFuture::Init(\n");
                out.push_str("        |waker: std::task::Waker, r: (");
                self.params.iter().for_each(|p| out.push_str(&p.rs_type()));
                out.push_str("), slot: *const (), cb: *const ()| {\n");
                out.push_str("let (_, waker_ref) = ::rust2go::ToRef::calc_ref(&waker);\n");
                out.push_str("std::mem::forget(waker);\n\n");
                out.push_str("let size = ::rust2go::ToRef::calc_size(&r);\n");
                out.push_str("let mut buffer = ::std::vec::Vec::<u8>::with_capacity(size);\n");
                out.push_str(
                    "let mut writer = unsafe { ::rust2go::Writer::new(buffer.as_ptr() as _) };\n",
                );
                out.push_str("let r_ref = ::rust2go::ToRef::to_ref(&r, &mut writer);\n");
                out.push_str("unsafe { buffer.set_len(size) };\n");

                out.push_str(&format!(
                    "unsafe {{ binding::C{trait_name}_{}(\n",
                    self.name
                ));
                out.push_str("::std::mem::transmute(waker_ref),\n");
                (0..self.params.len())
                    .for_each(|n| out.push_str(&format!("::std::mem::transmute(r_ref.{n}),")));
                out.push_str("                slot as *const _ as *mut _,\n");
                out.push_str("                cb as *const _ as *mut _,\n");
                out.push_str("            )}}, (");

                self.params
                    .iter()
                    .for_each(|p| out.push_str(&format!("{},", p.name)));

                out.push_str(&format!("),Self::{}_cb as *const ())}}", self.name));
            }
        }
        out
    }

    fn generate_rs_callback(&self, mapping: &HashMap<String, String>) -> String {
        let mut out = String::new();
        let fn_name = format!("{}_cb", self.name);
        match (self.is_async, &self.ret) {
            (true, None) => panic!("async function must have a return value"),
            (false, None) => {
                // There's no need to generate callback for sync function without callback.
            }
            (false, Some(ret)) => {
                // #[no_mangle]
                // unsafe extern "C" fn demo_check_cb(resp: binding::DemoResponseRef, slot: *const ()) {
                //     *(slot as *mut Option<DemoResponse>) = Some(::rust2go::FromRef::from_ref(::std::mem::transmute(&resp)));
                // }
                let resp_ref_ty = match mapping.get(ret) {
                    Some(ref_struct) => ref_struct.clone(),
                    None => ret.clone(),
                };
                out.push_str(&format!("#[no_mangle]\nunsafe extern \"C\" fn {fn_name}(resp: binding::{resp_ref_ty}, slot: *const ()) {{\n"));
                out.push_str(&format!("*(slot as *mut Option<{ret}>) = Some(::rust2go::FromRef::from_ref(::std::mem::transmute(&resp)));\n"));
                out.push_str("}\n");
            }
            (true, Some(ret)) => {
                // #[no_mangle]
                // unsafe extern "C" fn demo_check_async_cb(
                //     waker: binding::WakerRef,
                //     resp: binding::DemoResponseRef,
                //     slot: *const (),
                // ) {
                //     ::rust2go::SlotWriter::from_ptr(slot).write(::rust2go::RefConvertion::get_owned(&resp));
                //     ::rust2go::RefConvertion::get_owned(&waker).wake();
                //     ::rust2go::SlotWriter::<DemoResponse>::from_ptr(slot).write(::rust2go::FromRef::from_ref(::std::mem::transmute(&resp)));
                //     <::std::task::Waker as ::rust2go::FromRef>::from_ref(::std::mem::transmute(&waker)).wake();
                // }
                let resp_ref_ty = match mapping.get(ret) {
                    Some(ref_struct) => ref_struct.clone(),
                    None => ret.clone(),
                };
                out.push_str(&format!("#[no_mangle]\nunsafe extern \"C\" fn {fn_name}(waker: binding::WakerRef, resp: binding::{resp_ref_ty}, slot: *const ()) {{\n"));
                out.push_str(&format!("::rust2go::SlotWriter::<{ret}>::from_ptr(slot).write(::rust2go::FromRef::from_ref(::std::mem::transmute(&resp)));\n"));
                out.push_str("<::std::task::Waker as ::rust2go::FromRef>::from_ref(::std::mem::transmute(&waker)).wake();\n");
                out.push_str("}\n");
            }
        }
        out
    }
}

fn rust_primitive_to_c(name: &str) -> &str {
    // Ref: https://github.com/mozilla/cbindgen/blob/master/docs.md#std-types
    match name {
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
}

fn type_to_segment(ty: &syn::Type) -> anyhow::Result<&syn::PathSegment> {
    let field_type = match ty {
        syn::Type::Path(p) => p,
        _ => anyhow::bail!("only path types are supported"),
    };
    let path = &field_type.path;
    // Leading colon is not allow
    if path.leading_colon.is_some() {
        anyhow::bail!("types with leading colons are not supported");
    }
    // We only accept single-segment path
    if path.segments.len() != 1 {
        anyhow::bail!("types with multiple segments are not supported");
    }
    Ok(path.segments.first().unwrap())
}

fn type_to_string(ty: &syn::Type) -> anyhow::Result<String> {
    let seg = type_to_segment(ty)?;
    match seg.ident.to_string().as_str() {
        "Vec" => Ok(ty.to_token_stream().to_string()),
        _ => {
            if !seg.arguments.is_none() {
                anyhow::bail!("custom types with arguments are not supported")
            }
            Ok(seg.ident.to_string())
        }
    }
}

// Convert original type to ref type ident(only Vec can have generic type).
// primitive -> primitive
// Vec<T> -> ListRef
// * -> *Ref
fn type_to_ref_ident(ty: &syn::Type) -> anyhow::Result<syn::Ident> {
    let seg = type_to_segment(ty)?;
    match seg.ident.to_string().as_str() {
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "bool" | "char" | "f32" => {
            if !seg.arguments.is_none() {
                anyhow::bail!("primitive types with arguments are not supported");
            }
            Ok(seg.ident.clone())
        }
        "Vec" => Ok(syn::Ident::new("ListRef", seg.ident.span())),
        x => {
            if !seg.arguments.is_none() {
                anyhow::bail!("custom types with arguments are not supported")
            }
            Ok(syn::Ident::new(&format!("{x}Ref"), seg.ident.span()))
        }
    }
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
        let (names, result) = raw_file.convert_to_ref().unwrap();
        println!("names: {names:?}");
        println!("result: {result}");

        let traits = raw_file.convert_trait().unwrap();
        println!("traits: {traits:?}");

        for trait_ in traits {
            println!("traits gen: {}", trait_.generate_rs(&names, None));
        }

        bindgen::Builder::default()
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
            .generate()
            .expect("Unable to generate bindings");
    }
}
