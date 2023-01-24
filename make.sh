#! /usr/bin/env bash

# Build the project

echo 'Installing toolchains'
rustup toolchain add nightly
rustup target add aarch64-unknown-linux-gnu
rustup target add x86_64-unknown-linux-gnu
rustup target add x86_64-pc-windows-gnu

rm -r releases &> /dev/null
mkdir releases
cd releases


echo 'Packaging aarch64-unknown-linux-gnu'
cargo build --release --target=aarch64-unknown-linux-gnu
mkdir aarch64-unknown-linux-gnu
cp ../target/aarch64-unknown-linux-gnu/release/mcsc-server aarch64-unknown-linux-gnu/
cp ../target/aarch64-unknown-linux-gnu/release/mcsc-client aarch64-unknown-linux-gnu/
cp ../mcsc_client.toml aarch64-unknown-linux-gnu/
cp ../mcsc_server.toml aarch64-unknown-linux-gnu/
tar -czf aarch64-unknown-linux-gnu.tar.gz aarch64-unknown-linux-gnu
rm -r aarch64-unknown-linux-gnu

echo 'Packaging x86_64-unknown-linux-gnu'
cargo build --release --target=x86_64-unknown-linux-gnu
mkdir x86_64-unknown-linux-gnu
cp ../target/x86_64-unknown-linux-gnu/release/mcsc-server x86_64-unknown-linux-gnu/
cp ../target/x86_64-unknown-linux-gnu/release/mcsc-client x86_64-unknown-linux-gnu/
cp ../mcsc_client.toml x86_64-unknown-linux-gnu/
cp ../mcsc_server.toml x86_64-unknown-linux-gnu/
tar -czf x86_64-unknown-linux-gnu.tar.gz x86_64-unknown-linux-gnu
rm -r x86_64-unknown-linux-gnu

echo 'Packaging x86_64-pc-windows-gnu'
cargo build --release --target=x86_64-pc-windows-gnu
mkdir x86_64-pc-windows-gnu
cp ../target/x86_64-pc-windows-gnu/release/mcsc-client.exe x86_64-pc-windows-gnu/
cp ../target/x86_64-pc-windows-gnu/release/mcsc-client.exe x86_64-pc-windows-gnu/
cp ../mcsc_client.toml x86_64-pc-windows-gnu/
cp ../mcsc_server.toml x86_64-pc-windows-gnu/
tar -czf x86_64-pc-windows-gnu.tar.gz x86_64-pc-windows-gnu
rm -r x86_64-pc-windows-gnu

echo 'Done'