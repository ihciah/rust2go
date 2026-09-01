// Copyright 2024 ihciah. All Rights Reserved.

use std::collections::HashMap;

use quote::format_ident;
use syn::Ident;

use super::{BoolMark, R2GFnRepr, R2GTraitRepr};

impl R2GTraitRepr {
    // Generate golang exports.
    pub fn generate_go_exports(&self, levels: &HashMap<Ident, u8>) -> String {
        let name = self.name.to_string();
        let mut out: String = self
            .fns
            .iter()
            .map(|f| f.to_go_export(&name, levels))
            .collect();
        let shm_cnt = self.fns.iter().filter(|f| f.mem_call_id.is_some()).count();
        if shm_cnt != 0 {
            let mem_ffi_handles = (0..shm_cnt)
                .map(|id| format!("ringHandle{name}{id}"))
                .collect::<Vec<String>>();
            out.push_str(&format!("//export RingsInit{name}\nfunc RingsInit{name}(crr, crw C.QueueMeta) {{\nringsInit(crr, crw, []func(unsafe.Pointer, *ants.MultiPool, func(interface{{}}, []byte, uint)){{{}}})\n}}\n", mem_ffi_handles.join(",")));
        }
        out
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
}

impl R2GFnRepr {
    fn to_go_export(&self, trait_name: &str, levels: &HashMap<Ident, u8>) -> String {
        let ref_mark = BoolMark::new(self.go_ptr, "&");
        if let Some(mem_call_id) = self.mem_call_id {
            let fn_sig = format!("func ringHandle{trait_name}{mem_call_id}(ptr unsafe.Pointer, pool *ants.MultiPool, post_func func(interface{{}}, []byte, uint)) {{\n");
            let Some(ret) = &self.ret else {
                return format!("{fn_sig}post_func(nil, nil, 0)\n}}\n");
            };

            let mut fn_body = String::new();
            let params_len = self.params().len();
            for (idx, p) in self.params().iter().enumerate() {
                fn_body.push_str(&format!(
                    "{name}:=*(*C.{ref_type})(ptr)\n",
                    name = p.name,
                    ref_type = p.ty.to_c(false)
                ));
                if idx + 1 != params_len {
                    fn_body.push_str(&format!(
                        "ptr=unsafe.Pointer(uintptr(ptr)+unsafe.Sizeof({name}))\n",
                        name = p.name
                    ));
                }
                fn_body.push_str(&format!(
                    "{name}_:={cvt}({name})\n",
                    name = p.name,
                    cvt = p.ty.c_to_go_field_converter(levels).0
                ));
            }
            fn_body.push_str("pool.Submit(func() {\n");
            fn_body.push_str(&format!(
                "resp := {trait_name}Impl.{fn_name}({params})\n",
                fn_name = self.name,
                params = self
                    .params
                    .iter()
                    .map(|p| format!("{ref_mark}{}_", p.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            fn_body.push_str(&format!(
                "resp_ref_size := uint(unsafe.Sizeof(C.{}{{}}))\n",
                ret.to_c(false)
            ));
            let (g2c_cnt, g2c_cvt) = (
                ret.go_to_c_field_counter(levels).0,
                ret.go_to_c_field_converter(levels).0,
            );
            fn_body.push_str(&format!("resp_ref, buffer := cvt_ref_cap({g2c_cnt}, {g2c_cvt}, resp_ref_size)(&resp)\noffset := uint(len(buffer))\nbuffer = append(buffer, unsafe.Slice((*byte)(unsafe.Pointer(&resp_ref)), resp_ref_size)...)\n"));
            fn_body.push_str("post_func(resp, buffer, offset)\n})\n");
            let fn_ending = "}\n";
            return format!("{fn_sig}{fn_body}{fn_ending}");
        }

        let mut out = String::new();
        let fn_name = format!("C{}_{}", trait_name, self.name);
        out.push_str(&format!("//export {fn_name}\nfunc {fn_name}("));
        self.params
            .iter()
            .for_each(|p| out.push_str(&format!("{} C.{}, ", p.name, p.ty.to_c(false))));

        let mut new_names = Vec::new();
        let mut new_cvt = String::new();
        for p in self.params.iter() {
            let new_name = format_ident!("_new_{}", p.name);
            let cvt = p.ty.c_to_go_field_converter(levels).0;
            new_cvt.push_str(&format!("{new_name} := {cvt}({})\n", p.name));
            new_names.push(format!("{ref_mark}{new_name}"));
        }
        match (self.is_async, &self.ret) {
            (true, None) => panic!("async function must have a return value"),
            (false, None) => {
                // //export CDemoCall_demo_oneway
                // func CDemoCall_demo_oneway(req C.DemoUserRef) {
                //     DemoCallImpl.demo_oneway(newDemoUser(req))
                // }
                out.push_str(") {\n");
                out.push_str(&new_cvt);
                out.push_str(&format!(
                    "    {trait_name}Impl.{fn_name}({params})\n",
                    fn_name = self.name,
                    params = new_names.join(", ")
                ));
                out.push_str("}\n");
            }
            (false, Some(ret)) => {
                // //export CDemoCall_demo_check
                // func CDemoCall_demo_check(req C.DemoComplicatedRequestRef, slot *C.void, cb *C.void) {
                //     resp := DemoCallImpl.demo_check(newDemoComplicatedRequest(req))
                //     resp_ref, buffer := cvt_ref(cntDemoResponse, refDemoResponse)(&resp)
                //     C.DemoCall_demo_check_cb(unsafe.Pointer(cb), &resp_ref, unsafe.Pointer(slot))
                //     runtime.KeepAlive(resp_ref)
                //     runtime.KeepAlive(resp)
                //     runtime.KeepAlive(buffer)
                // }
                out.push_str("slot *C.void, cb *C.void) {\n");
                out.push_str(&new_cvt);
                out.push_str(&format!(
                    "resp := {trait_name}Impl.{fn_name}({params})\n",
                    fn_name = self.name,
                    params = new_names.join(", ")
                ));
                let (g2c_cnt, g2c_cvt) = (
                    ret.go_to_c_field_counter(levels).0,
                    ret.go_to_c_field_converter(levels).0,
                );
                out.push_str(&format!(
                    "resp_ref, buffer := cvt_ref({g2c_cnt}, {g2c_cvt})(&resp)\n"
                ));
                if self.cgo_cb {
                    out.push_str("cgocall.CallFuncG0P2(unsafe.Pointer(cb), unsafe.Pointer(&resp_ref), unsafe.Pointer(slot))\n");
                } else {
                    out.push_str("asmcall.CallFuncG0P2(unsafe.Pointer(cb), unsafe.Pointer(&resp_ref), unsafe.Pointer(slot))\n");
                }
                out.push_str("runtime.KeepAlive(resp_ref)\nruntime.KeepAlive(resp)\nruntime.KeepAlive(buffer)\n");
                out.push_str("}\n");
            }
            (true, Some(ret)) => {
                // //export CDemoCall_demo_check_async
                // func CDemoCall_demo_check_async(req C.DemoComplicatedRequestRef, slot *C.void, cb *C.void) {
                //     _new_req := newDemoComplicatedRequest(req)
                //     go func() {
                //         resp := DemoCallImpl.demo_check_async(_new_req)
                //         resp_ref, buffer := cvt_ref(cntDemoResponse, refDemoResponse)(&resp)
                //         C.DemoCall_demo_check_async_cb(unsafe.Pointer(cb), &resp_ref, unsafe.Pointer(slot))
                //         runtime.KeepAlive(resp)
                //         runtime.KeepAlive(resp)
                //         runtime.KeepAlive(buffer)
                //     }()
                // }
                out.push_str("slot *C.void, cb *C.void) {\n");
                out.push_str(&new_cvt);
                out.push_str("    go func() {\n");
                out.push_str(&format!(
                    "resp := {trait_name}Impl.{fn_name}({params})\n",
                    fn_name = self.name,
                    params = new_names.join(", ")
                ));
                let (g2c_cnt, g2c_cvt) = (
                    ret.go_to_c_field_counter(levels).0,
                    ret.go_to_c_field_converter(levels).0,
                );
                out.push_str(&format!(
                    "resp_ref, buffer := cvt_ref({g2c_cnt}, {g2c_cvt})(&resp)\n"
                ));
                if self.cgo_cb {
                    out.push_str("cgocall.CallFuncG0P2(unsafe.Pointer(cb), unsafe.Pointer(&resp_ref), unsafe.Pointer(slot))\n");
                } else {
                    out.push_str("asmcall.CallFuncG0P2(unsafe.Pointer(cb), unsafe.Pointer(&resp_ref), unsafe.Pointer(slot))\n");
                }
                out.push_str("runtime.KeepAlive(resp_ref)\nruntime.KeepAlive(resp)\nruntime.KeepAlive(buffer)\n");
                out.push_str("}()\n}\n");
            }
        }
        out
    }

    fn to_go_interface_method(&self) -> String {
        // demo_oneway(req DemoUser)
        // demo_check(req DemoComplicatedRequest) DemoResponse
        let star_mark = BoolMark::new(self.go_ptr, "*");
        format!(
            "{}({}) {}",
            self.name,
            self.params
                .iter()
                .map(|p| format!("{} {star_mark}{}", p.name, p.ty.to_go()))
                .collect::<Vec<_>>()
                .join(", "),
            self.ret.as_ref().map(|p| p.to_go()).unwrap_or_default()
        )
    }
}
