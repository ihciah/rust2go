// Copyright 2024 ihciah. All Rights Reserved.

use std::collections::HashMap;

use syn::Ident;

use super::G2RTraitRepr;

impl G2RTraitRepr {
    pub fn to_go(&self, levels: &HashMap<Ident, u8>) -> String {
        let trait_name = &self.name;
        let struct_name = format!("{trait_name}Impl");
        let mut out = format!("type {struct_name} struct{{}}\n");

        for f in &self.fns {
            let call_type = if f.cgo_call { "cgocall" } else { "asmcall" };
            let ffi_param_cnt = f.ffi_param_cnt();
            let f_name = &f.name;

            let params = f
                .params
                .iter()
                .map(|p| format!("{} *{}", p.name, p.ty.to_go()))
                .collect::<Vec<_>>()
                .join(",");
            let ret = f.ret.as_ref().map_or(String::new(), |ret| ret.to_go());
            let init_slot = or_empty!(f.ret.is_some(), "_internal_slot := [2]unsafe.Pointer{}\n");
            let mut init_params = String::new();
            if !f.params.is_empty() {
                init_params = format!(
                    "_internal_params := [{}]unsafe.Pointer{{}}\n",
                    f.params.len()
                );
            }

            // write function header
            out.push_str(&format!(
                "func ({struct_name}) {f_name}({params}) {ret} {{
                    {init_slot}{init_params}"
            ));

            // convert params
            for (i, p) in f.params.iter().enumerate() {
                // user_ref, user_buffer := cvt_ref(cntDemoUser, refDemoUser)(user)
                // _internal_params[0] = unsafe.Pointer(&user_ref)
                let cnt = p.ty.go_to_c_field_counter(levels).0;
                let ref_ = p.ty.go_to_c_field_converter(levels).0;
                out.push_str(&format!(
                    "{pname}_ref, {pname}_buffer := cvt_ref({cnt}, {ref_})({pname})
                    _internal_params[{i}] = unsafe.Pointer(&{pname}_ref)
                    ",
                    pname = p.name,
                ));
            }

            // call
            let mut call_params = String::new();
            // unsafe.Pointer(&_internal_slot), unsafe.Pointer(&_internal_params)
            if f.ret.is_some() {
                call_params.push_str(", unsafe.Pointer(&_internal_slot)");
            }
            if !f.params.is_empty() {
                call_params.push_str(", unsafe.Pointer(&_internal_params)");
            }
            out.push_str(&format!(
                "{call_type}.CallFuncG0P{ffi_param_cnt}(unsafe.Pointer(C.c_{trait_name}_{f_name}){call_params})\n"
            ));

            // keepalive
            if f.ret.is_some() {
                out.push_str("runtime.KeepAlive(_internal_slot)\n");
            }
            if !f.params.is_empty() {
                out.push_str("runtime.KeepAlive(_internal_params)\n");
            }
            for p in f.params.iter() {
                out.push_str(&format!("runtime.KeepAlive({}_buffer)\n", p.name));
            }

            if let Some(r) = &f.ret {
                // val := ownString(*(*C.StringRef)(_internal_slot[0]))
                // asmcall.CallFuncG0P1(unsafe.Pointer(C.c_rust2go_internal_drop), unsafe.Pointer(_internal_slot[1]))
                // return val
                let cvt = r.c_to_go_field_converter_owned();
                let cty = r.to_c(false);
                out.push_str(&format!("val := {cvt}(*(*C.{cty})(_internal_slot[0]))
                {call_type}.CallFuncG0P1(unsafe.Pointer(C.c_rust2go_internal_drop), unsafe.Pointer(_internal_slot[1]))
                return val
                "));
            }

            out.push_str("}\n");
        }

        out
    }
}
