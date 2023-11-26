use std::{
    env,
    path::{Path, PathBuf},
};

use crate::raw_file::RawRsFile;

pub struct Builder {
    idl: PathBuf,
    out_name: String,
    out_dir: Option<PathBuf>,
}

impl Builder {
    pub fn build(self) {
        let file_content = std::fs::read_to_string(self.idl).expect("Unable to read file");

        let raw_file = RawRsFile::new(file_content);
        let (mapping, _) = raw_file.convert_to_ref().expect("Unable to convert to ref");
        let traits = raw_file.convert_trait().expect("Parse trait failed");

        let mut output = String::new();
        for tt in traits.iter() {
            output.push_str(&tt.generate_rs(&mapping));
        }

        let out_dir = self
            .out_dir
            .unwrap_or_else(|| PathBuf::from(env::var("OUT_DIR").unwrap()));
        let out_file = out_dir.join(self.out_name);
        std::fs::write(out_file, output).expect("Unable to write file");
    }
}
