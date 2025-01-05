#!/bin/sh

echo "Building example-go2rust with statically link"

# build rust-lib
cd rust-lib || exit
cargo build --release || exit
cd .. || exit

# copy output
# [NOTE] You may have to adjust the path by your own!
cp ../../target/release/librust_lib.a ./

# build go
go build .
