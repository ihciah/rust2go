// Copyright 2024 ihciah. All Rights Reserved.

use clap::Parser;
use rust2go_cli::{generate, Args};

fn main() {
    let args = Args::parse();
    generate(&args);
}
