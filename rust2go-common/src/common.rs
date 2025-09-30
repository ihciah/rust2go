// Copyright 2024 ihciah. All Rights Reserved.

use crate::{g2r::G2RTraitRepr, r2g::R2GTraitRepr};
use heck::{
    ToKebabCase, ToLowerCamelCase, ToShoutyKebabCase, ToShoutySnakeCase, ToSnakeCase,
    ToTitleCase, ToTrainCase, ToUpperCamelCase
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use std::collections::HashMap;
use syn::parse::Parser;
use syn::{
    Attribute, Error, Expr, ExprLit, File, Ident, Item, Lit, Meta, MetaNameValue, PathSegment,
    Result, Type,
};

pub struct RawRsFile {
    file: File,
}

impl RawRsFile {
    pub fn new<S: AsRef<str>>(src: S) -> Self {
        let src = src.as_ref();
        let syntax = syn::parse_file(src).expect("Unable to parse file");
        RawRsFile { file: syntax }
    }

    pub fn go_internal_drop() -> &'static str {
        r#"
const void c_rust2go_internal_drop(void*);
"#
    }

    pub fn go_shm_include() -> &'static str {
        r#"
typedef struct QueueMeta {
    uintptr_t buffer_ptr;
    uintptr_t buffer_len;
    uintptr_t head_ptr;
    uintptr_t tail_ptr;
    uintptr_t working_ptr;
    uintptr_t stuck_ptr;
    int32_t working_fd;
    int32_t unstuck_fd;
    } QueueMeta;
"#
    }

    pub fn go_shm_ring_init() -> &'static str {
        r#"
        func ringsInit(crr, crw C.QueueMeta, fns []func(unsafe.Pointer, *ants.MultiPool, func(interface{}, []byte, uint))) {
            const MULTIPOOL_SIZE = 8
            const SIZE_PER_POOL = -1

            type Storage struct {
                resp   interface{}
                buffer []byte
            }

            type Payload struct {
                Ptr          uint
                UserData     uint
                NextUserData uint
                CallId       uint32
                Flag         uint32
            }

            const CALL = 0b0101
            const REPLY = 0b1110
            const DROP = 0b1000

            queueMetaCvt := func(cq C.QueueMeta) mem_ring.QueueMeta {
                return mem_ring.QueueMeta{
                    BufferPtr:  uintptr(cq.buffer_ptr),
                    BufferLen:  uintptr(cq.buffer_len),
                    HeadPtr:    uintptr(cq.head_ptr),
                    TailPtr:    uintptr(cq.tail_ptr),
                    WorkingPtr: uintptr(cq.working_ptr),
                    StuckPtr:   uintptr(cq.stuck_ptr),
                    WorkingFd:  int32(cq.working_fd),
                    UnstuckFd:  int32(cq.unstuck_fd),
                }
            }

            rr := queueMetaCvt(crr)
            rw := queueMetaCvt(crw)

            rrq := mem_ring.NewQueue[Payload](rr)
            rwq := mem_ring.NewQueue[Payload](rw)

            gr := rwq.Read()
            gw := rrq.Write()

            slab := mem_ring.NewMultiSlab[Storage]()
            pool, _ := ants.NewMultiPool(MULTIPOOL_SIZE, SIZE_PER_POOL, ants.RoundRobin)

            gr.RunHandler(func(p Payload) {
                if p.Flag == CALL {
                    post_func := func(resp interface{}, buffer []byte, offset uint) {
                        if resp == nil {
                            payload := Payload{
                                Ptr:          0,
                                UserData:     p.UserData,
                                NextUserData: 0,
                                CallId:       p.CallId,
                                Flag:         DROP,
                            }
                            gw.Push(payload)
                            return
                        }

                        // Use slab to hold reference of resp and buffer
                        sid := slab.Push(Storage{
                            resp,
                            buffer,
                        })
                        payload := Payload{
                            Ptr:          uint(uintptr(unsafe.Pointer(&buffer[offset]))),
                            UserData:     p.UserData,
                            NextUserData: sid,
                            CallId:       p.CallId,
                            Flag:         REPLY,
                        }
                        gw.Push(payload)
                    }
                    fns[p.CallId](unsafe.Pointer(uintptr(p.Ptr)), pool, post_func)
                } else if p.Flag == DROP {
                    // drop memory instantly
                    slab.Pop(p.UserData)
                }
            })
        }
        "#
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
        func refString(s *string, _ *[]byte) C.StringRef {
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
        func refString(s *string, _ *[]byte) C.StringRef {
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
        func ownString(s_ref C.StringRef) string {
            return string(unsafe.Slice((*byte)(unsafe.Pointer(s_ref.ptr)), int(s_ref.len)))
        }
        func cntString(_ *string, _ *uint) [0]C.StringRef { return [0]C.StringRef{} }
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
        func new_list_mapper_primitive[T1, T2 any](_ func(T1) T2) func(C.ListRef) []T2 {
            return func(x C.ListRef) []T2 {
                return unsafe.Slice((*T2)(unsafe.Pointer(x.ptr)), x.len)
            }
        }
        // only handle non-primitive type T
        func cnt_list_mapper[T, R any](f func(s *T, cnt *uint)[0]R) func(s *[]T, cnt *uint) [0]C.ListRef {
            return func(s *[]T, cnt *uint) [0]C.ListRef {
                for _, v := range *s {
                    f(&v, cnt)
                }
                *cnt += uint(len(*s)) * size_of[R]()
                return [0]C.ListRef{}
            }
        }

        // only handle primitive type T
        func cnt_list_mapper_primitive[T, R any](_ func(s *T, cnt *uint)[0]R) func(s *[]T, cnt *uint) [0]C.ListRef {
            return func(s *[]T, cnt *uint) [0]C.ListRef {return [0]C.ListRef{}}
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
        func ref_list_mapper_primitive[T, R any](_ func(s *T, buffer *[]byte) R) func(s *[]T, buffer *[]byte) C.ListRef {
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
        func cvt_ref[R, CR any](cnt_f func(s *R, cnt *uint) [0]CR, ref_f func(p *R, buffer *[]byte) CR) func(p *R) (CR, []byte) {
            return func(p *R) (CR, []byte) {
                var cnt uint
                cnt_f(p, &cnt)
                buffer := make([]byte, cnt)
                return ref_f(p, &buffer), buffer
            }
        }
        func cvt_ref_cap[R, CR any](cnt_f func(s *R, cnt *uint) [0]CR, ref_f func(p *R, buffer *[]byte) CR, add_cap uint) func(p *R) (CR, []byte) {
            return func(p *R) (CR, []byte) {
                var cnt uint
                cnt_f(p, &cnt)
                buffer := make([]byte, cnt, cnt + add_cap)
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

        func cntC_uint8_t(_ *uint8, _ *uint) [0]C.uint8_t    { return [0]C.uint8_t{} }
        func cntC_uint16_t(_ *uint16, _ *uint) [0]C.uint16_t { return [0]C.uint16_t{} }
        func cntC_uint32_t(_ *uint32, _ *uint) [0]C.uint32_t { return [0]C.uint32_t{} }
        func cntC_uint64_t(_ *uint64, _ *uint) [0]C.uint64_t { return [0]C.uint64_t{} }
        func cntC_int8_t(_ *int8, _ *uint) [0]C.int8_t       { return [0]C.int8_t{} }
        func cntC_int16_t(_ *int16, _ *uint) [0]C.int16_t    { return [0]C.int16_t{} }
        func cntC_int32_t(_ *int32, _ *uint) [0]C.int32_t    { return [0]C.int32_t{} }
        func cntC_int64_t(_ *int64, _ *uint) [0]C.int64_t    { return [0]C.int64_t{} }
        func cntC_bool(_ *bool, _ *uint) [0]C.bool           { return [0]C.bool{} }
        func cntC_uintptr_t(_ *uint, _ *uint) [0]C.uintptr_t { return [0]C.uintptr_t{} }
        func cntC_intptr_t(_ *int, _ *uint) [0]C.intptr_t    { return [0]C.intptr_t{} }
        func cntC_float(_ *float32, _ *uint) [0]C.float      { return [0]C.float{} }
        func cntC_double(_ *float64, _ *uint) [0]C.double    { return [0]C.double{} }

        func refC_uint8_t(p *uint8, _ *[]byte) C.uint8_t    { return C.uint8_t(*p) }
        func refC_uint16_t(p *uint16, _ *[]byte) C.uint16_t { return C.uint16_t(*p) }
        func refC_uint32_t(p *uint32, _ *[]byte) C.uint32_t { return C.uint32_t(*p) }
        func refC_uint64_t(p *uint64, _ *[]byte) C.uint64_t { return C.uint64_t(*p) }
        func refC_int8_t(p *int8, _ *[]byte) C.int8_t       { return C.int8_t(*p) }
        func refC_int16_t(p *int16, _ *[]byte) C.int16_t    { return C.int16_t(*p) }
        func refC_int32_t(p *int32, _ *[]byte) C.int32_t    { return C.int32_t(*p) }
        func refC_int64_t(p *int64, _ *[]byte) C.int64_t    { return C.int64_t(*p) }
        func refC_bool(p *bool, _ *[]byte) C.bool           { return C.bool(*p) }
        func refC_uintptr_t(p *uint, _ *[]byte) C.uintptr_t { return C.uintptr_t(*p) }
        func refC_intptr_t(p *int, _ *[]byte) C.intptr_t    { return C.intptr_t(*p) }
        func refC_float(p *float32, _ *[]byte) C.float      { return C.float(*p) }
        func refC_double(p *float64, _ *[]byte) C.double    { return C.double(*p) }
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
                            Self::gen_tag(&field_name, &go_struct_tag)
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
            .map(|trat| trat.try_into())
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
            .map(|trat| trat.try_into())
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
                "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize"
                | "bool" | "char" | "f32" | "f64" => Ok(Node::Primitive),
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

    fn gen_tag(field_name: &str, tag_list: &[(String, String)]) -> String {
        let mut tags = vec![];
        for (key, heck_type) in tag_list {
            tags.push(format!(
                "{}:{:?}",
                key,
                Self::heck_field_name(field_name, heck_type)
            ));
        }
        if tags.is_empty() {
            return String::new();
        }
        format!("`{}`", tags.join(" "))
    }

    fn heck_field_name(field_name: &str, heck_type: &str) -> String {
        match heck_type {
            "snake_case" => field_name.to_snake_case(),
            "lowerCamelCase" => field_name.to_lower_camel_case(),
            "UpperCamelCase" => field_name.to_upper_camel_case(),
            "kebab-case" => field_name.to_kebab_case(),
            "SHOUTY_SNAKE_CASE" => field_name.to_shouty_snake_case(),
            "SHOUTY-KEBAB-CASE" => field_name.to_shouty_kebab_case(),
            "Title Case" => field_name.to_title_case(),
            "Train-Case" => field_name.to_train_case(),
            _ => panic!("unknown heck type"),
        }
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
            "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "usize" | "isize"
            | "bool" | "char" | "f32" | "f64" => {
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
    pub fn to_c(&self, with_struct: bool) -> String {
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

    pub fn to_go(&self) -> String {
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
    pub fn c_to_go_field_converter(&self, mapping: &HashMap<Ident, u8>) -> (String, u8) {
        match &self.inner {
            ParamTypeInner::Primitive(name) => (
                match name.to_string().as_str() {
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
                    _ => panic!("unrecognized rust primitive type {name}"),
                }
                .to_string(),
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
                _ => panic!("unrecognized rust primitive type {name}"),
            }
            .to_string(),
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
                    _ => panic!("unrecognized rust primitive type {name}"),
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
    pub fn go_to_c_field_converter(&self, mapping: &HashMap<Ident, u8>) -> (String, u8) {
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
}
