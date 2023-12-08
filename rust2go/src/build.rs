use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

use crate::raw_file::RawRsFile;

#[derive(PartialEq, Eq)]
pub enum LinkType {
    Static,
    Dynamic,
}

pub struct Builder<IDL = (), GOSRC = ()> {
    idl: IDL,
    go_src: GOSRC,
    out_dir: Option<PathBuf>,
    out_name: Option<String>,
    log: Option<PathBuf>,
    binding_name: Option<String>,
    link: LinkType,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Builder {
            idl: (),
            go_src: (),
            out_dir: None,
            out_name: None,
            log: None,
            binding_name: None,
            link: LinkType::Static,
        }
    }
}

impl<IDL, GOSRC> Builder<IDL, GOSRC> {
    pub fn with_rs_idl<S: Into<PathBuf>>(self, idl: S) -> Builder<PathBuf, GOSRC> {
        Builder {
            idl: idl.into(),
            go_src: self.go_src,
            out_name: self.out_name,
            out_dir: self.out_dir,
            log: self.log,
            binding_name: self.binding_name,
            link: self.link,
        }
    }

    pub fn with_go_src<S: Into<PathBuf>>(self, go_src: S) -> Builder<IDL, PathBuf> {
        Builder {
            idl: self.idl,
            go_src: go_src.into(),
            out_name: self.out_name,
            out_dir: self.out_dir,
            log: self.log,
            binding_name: self.binding_name,
            link: self.link,
        }
    }

    pub fn with_log<S: Into<PathBuf>>(self, log: S) -> Self {
        Builder {
            idl: self.idl,
            go_src: self.go_src,
            out_name: self.out_name,
            out_dir: self.out_dir,
            log: Some(log.into()),
            binding_name: self.binding_name,
            link: self.link,
        }
    }

    pub fn with_link(self, link: LinkType) -> Self {
        Builder {
            idl: self.idl,
            go_src: self.go_src,
            out_name: self.out_name,
            out_dir: self.out_dir,
            log: self.log,
            binding_name: self.binding_name,
            link,
        }
    }
}

impl Builder<PathBuf, PathBuf> {
    pub fn build(self) {
        // Read and parse IDL file.
        let file_content = std::fs::read_to_string(self.idl).expect("Unable to read file");
        let raw_file = RawRsFile::new(file_content);
        let traits = raw_file.convert_trait().expect("Parse trait failed");

        // Golang -> $OUT_DIR/_go_bindings.rs
        // This file must be in OUT_DIR, not user specified
        // File name can be specified by users
        let binding_name = self
            .binding_name
            .as_deref()
            .unwrap_or(crate::DEFAULT_BINDING_NAME);
        Self::build_go(&self.go_src, binding_name, self.link);

        // Generate trait impls and ext impls
        let mut output = String::new();
        for tt in traits.iter() {
            output.push_str(&tt.generate_rs(Some(binding_name)).unwrap().to_string());
        }

        // Write into $OUT_DIR/rust2go.rs(dir and file name can be specified by users)
        let out_dir = self
            .out_dir
            .unwrap_or_else(|| PathBuf::from(env::var("OUT_DIR").unwrap()));
        let out_name = self.out_name.as_deref().unwrap_or("rust2go.rs");
        let out_file = out_dir.join(out_name);
        std::fs::write(out_file, &output).expect("Unable to write file");
        if let Some(log_file) = self.log {
            std::fs::write(log_file, output).expect("Unable to write log file");
        }
    }

    fn build_go(go_src: &Path, binding_name: &str, link: LinkType) {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let mut go_build = Command::new("go");
        go_build
            .arg("build")
            .arg(if link == LinkType::Static {
                "-buildmode=c-archive"
            } else {
                "-buildmode=c-shared"
            })
            .arg("-o")
            .arg(out_dir.join(if link == LinkType::Static {
                "libgo.a"
            } else {
                "libgo.so"
            }))
            .arg(go_src);

        go_build.status().expect("Go build failed");

        let bindings = bindgen::Builder::default()
            .header(out_dir.join("libgo.h").to_str().unwrap())
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
            .generate()
            .expect("Unable to generate bindings");

        bindings
            .write_to_file(out_dir.join(binding_name))
            .expect("Couldn't write bindings!");

        println!("cargo:rerun-if-changed={}", go_src.to_str().unwrap());
        println!(
            "cargo:rustc-link-search=native={}",
            out_dir.to_str().unwrap()
        );
        if link == LinkType::Static {
            println!("cargo:rustc-link-lib=static=go");
        } else {
            println!("cargo:rustc-link-lib=dylib=go");
        }
    }
}
