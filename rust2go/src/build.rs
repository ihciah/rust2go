// Copyright 2024 ihciah. All Rights Reserved.

use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

use rust2go_cli::Args;

/// Static lib extension on non-Windows platforms.
#[cfg(not(windows))]
const LIB_EXT: &str = ".a";
/// Static lib extension on Windows.
#[cfg(windows)]
const LIB_EXT: &str = ".lib";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkType {
    Static,
    Dynamic,
    Musl,
}

/// Builder is a builder for building rust2go.
pub struct Builder<GOSRC = (), GOC = CustomArgGoCompiler> {
    go_src: GOSRC,
    out_dir: Option<PathBuf>,
    binding_name: Option<String>,
    link: LinkType,
    regen_arg: Args,
    copy_lib: CopyLib,
    go_comp: GOC,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CopyLib {
    Disabled,
    DefaultPath,
    CustomPath(PathBuf),
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Builder {
            go_src: (),
            out_dir: None,
            binding_name: None,
            link: LinkType::Static,
            regen_arg: Args::default(),
            copy_lib: CopyLib::Disabled,
            go_comp: CustomArgGoCompiler::new(),
        }
    }

    pub fn musl() -> Self {
        Builder {
            go_src: (),
            out_dir: None,
            binding_name: None,
            link: LinkType::Musl,
            regen_arg: Args::default(),
            copy_lib: CopyLib::Disabled,
            go_comp: CustomArgGoCompiler::new(),
        }
    }
}

impl<GOSRC, GOC> Builder<GOSRC, GOC> {
    /// Set go src.
    pub fn with_go_src<S: Into<PathBuf>>(self, go_src: S) -> Builder<PathBuf, GOC> {
        Builder {
            go_src: go_src.into(),
            out_dir: self.out_dir,
            binding_name: self.binding_name,
            link: self.link,
            regen_arg: self.regen_arg,
            copy_lib: self.copy_lib,
            go_comp: self.go_comp,
        }
    }

    /// Set go compiler.
    pub fn with_go_compiler<GOC2>(self, go_comp: GOC2) -> Builder<GOSRC, GOC2> {
        Builder {
            go_src: self.go_src,
            out_dir: self.out_dir,
            binding_name: self.binding_name,
            link: self.link,
            regen_arg: self.regen_arg,
            copy_lib: self.copy_lib,
            go_comp,
        }
    }

    /// Get mutable reference to go compiler.
    pub fn go_compiler_mut(&mut self) -> &mut GOC {
        &mut self.go_comp
    }

    /// Default binding name is "_go_bindings.rs".
    /// Use with_binding to set it.
    pub fn with_binding(mut self, binding_name: impl Into<String>) -> Self {
        self.binding_name = Some(binding_name.into());
        self
    }

    /// Default link type is static linking.
    /// Use with_link to set it.
    pub fn with_link(mut self, link: LinkType) -> Self {
        self.link = link;
        self
    }

    /// Regenerate go code.
    /// Note: you should generate go code before build with rust2go-cli.
    /// This function is to make sure the go code is updated.
    pub fn with_regen(mut self, src: &str, dst: &str) -> Self {
        self.regen_arg.src = src.to_string();
        self.regen_arg.dst = dst.to_string();
        self
    }

    /// Regenerate go code.
    /// Note: you should generate go code before build with rust2go-cli.
    /// This function is to make sure the go code is updated.
    pub fn with_regen_arg(mut self, arg: Args) -> Self {
        self.regen_arg = arg;
        self
    }

    /// Copy DLL to target dir.
    pub fn with_copy_lib(mut self, copy_lib: CopyLib) -> Self {
        self.copy_lib = copy_lib;
        self
    }
}

impl<GOSRC> Builder<GOSRC, CustomArgGoCompiler> {
    /// Append argument to go compiler.
    /// This function only works with CustomArgGoCompiler.
    /// This is a shortcut for `.go_compiler_mut().arg(arg)`.
    pub fn compiler_arg(&mut self, arg: impl Into<OsString>) -> &mut Self {
        self.go_comp.arg(arg);
        self
    }

    /// Append environment variable to go compiler.
    /// This function only works with CustomArgGoCompiler.
    /// This is a shortcut for `.go_compiler_mut().env(key, val)`.
    pub fn compiler_env(
        &mut self,
        key: impl Into<OsString>,
        val: impl Into<OsString>,
    ) -> &mut Self {
        self.go_comp.env(key, val);
        self
    }
}

pub trait GoCompiler {
    fn go_build(&self, go_src: &Path, link: LinkType, output: &Path);

    fn build(&self, go_src: &Path, binding_name: &str, link: LinkType, copy_lib: &CopyLib) {
        use std::env::consts::DLL_PREFIX;

        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let out_filename = filename(link);
        let output = out_dir.join(&out_filename);

        self.go_build(go_src, link, output.as_path());

        // Copy the DLL file to target dir.
        if link == LinkType::Dynamic || link == LinkType::Musl {
            // A workaround to get target dir.
            // From https://github.com/rust-lang/cargo/issues/9661#issuecomment-1722358176
            fn get_cargo_target_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
                let out_dir = PathBuf::from(env::var("OUT_DIR")?);
                let profile = env::var("PROFILE")?;
                let mut target_dir = None;
                let mut sub_path = out_dir.as_path();
                while let Some(parent) = sub_path.parent() {
                    if parent.ends_with(&profile) {
                        target_dir = Some(parent);
                        break;
                    }
                    sub_path = parent;
                }
                let target_dir = target_dir.ok_or("not found")?;
                Ok(target_dir.to_path_buf())
            }

            match copy_lib {
                CopyLib::Disabled => (),
                CopyLib::DefaultPath => {
                    let target_dir = get_cargo_target_dir().unwrap();
                    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
                    std::fs::copy(out_dir.join(&out_filename), target_dir.join(&out_filename))
                        .expect("unable to copy dynamic library");
                }
                CopyLib::CustomPath(p) => {
                    std::fs::copy(out_dir.join(&out_filename), p.join(&out_filename))
                        .expect("unable to copy dynamic library");
                }
            }
        }

        let bindings = bindgen::Builder::default()
            .header(out_dir.join(format!("{DLL_PREFIX}go.h")).to_str().unwrap())
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

/// DefaultGoCompiler is a GoCompiler that uses default arguments.
#[derive(Debug, Clone, Copy)]
pub struct DefaultGoCompiler;
impl GoCompiler for DefaultGoCompiler {
    fn go_build(&self, go_src: &Path, link: LinkType, output: &Path) {
        let mut go_build = Command::new("go");

        if link == LinkType::Musl {
            go_build.env("CC", "musl-gcc");
        }

        go_build
            .env("GO111MODULE", "on")
            .current_dir(go_src)
            .arg("build")
            .arg(if link == LinkType::Static {
                "-buildmode=c-archive"
            } else {
                "-buildmode=c-shared"
            })
            // .arg(r#"-gcflags="all=-N -l""#)
            .arg("-o")
            .arg(output)
            .arg(".");

        assert!(
            go_build.status().expect("Go build failed").success(),
            "Go build failed"
        );
    }
}

/// CustomArgGoCompiler is a GoCompiler that allows users to customize arguments and environment variables.
#[derive(Debug, Clone)]
pub struct CustomArgGoCompiler {
    args: Vec<OsString>,
    envs: Vec<(OsString, OsString)>,
}
impl CustomArgGoCompiler {
    /// Create a new CustomArgGoCompiler.
    pub fn new() -> Self {
        Self {
            args: Vec::new(),
            envs: vec![("GO111MODULE".into(), "on".into())],
        }
    }
    /// Append argument to go compiler.
    pub fn arg(&mut self, arg: impl Into<OsString>) -> &mut Self {
        self.args.push(arg.into());
        self
    }
    /// Append environment variable to go compiler.
    pub fn env(&mut self, key: impl Into<OsString>, val: impl Into<OsString>) -> &mut Self {
        self.envs.push((key.into(), val.into()));
        self
    }
    /// Get mutable reference to arguments.
    pub fn args_mut(&mut self) -> &mut Vec<OsString> {
        &mut self.args
    }
    /// Get mutable reference to environment variables.
    pub fn envs_mut(&mut self) -> &mut Vec<(OsString, OsString)> {
        &mut self.envs
    }
}
impl Default for CustomArgGoCompiler {
    fn default() -> Self {
        Self::new()
    }
}
impl GoCompiler for CustomArgGoCompiler {
    fn go_build(&self, go_src: &Path, link: LinkType, output: &Path) {
        let mut go_build = Command::new("go");
        let mut cmd = &mut go_build;
        for (key, val) in &self.envs {
            cmd = cmd.env(key, val);
        }
        cmd.current_dir(go_src)
            .arg("build")
            .arg(if link == LinkType::Static {
                "-buildmode=c-archive"
            } else {
                "-buildmode=c-shared"
            });
        for arg in &self.args {
            cmd = cmd.arg(arg);
        }
        cmd.arg("-o").arg(output).arg(".");

        assert!(
            go_build.status().expect("Go build failed").success(),
            "Go build failed"
        );
    }
}

impl<GOC: GoCompiler> Builder<PathBuf, GOC> {
    pub fn build(self) {
        // Golang -> $OUT_DIR/_go_bindings.rs
        // This file must be in OUT_DIR, not user specified
        // File name can be specified by users
        let binding_name = self
            .binding_name
            .as_deref()
            .unwrap_or(crate::DEFAULT_BINDING_FILE);
        // Regenerate go code.
        if !self.regen_arg.src.is_empty() && !self.regen_arg.dst.is_empty() {
            rust2go_cli::generate(&self.regen_arg);
        }
        self.go_comp
            .build(&self.go_src, binding_name, self.link, &self.copy_lib);
    }
}

fn filename(link_type: LinkType) -> String {
    use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};

    match link_type {
        LinkType::Static => format!("{DLL_PREFIX}go{LIB_EXT}"),
        LinkType::Musl => format!("{DLL_PREFIX}go{LIB_EXT}"),
        LinkType::Dynamic => format!("{DLL_PREFIX}go{DLL_SUFFIX}"),
    }
}
