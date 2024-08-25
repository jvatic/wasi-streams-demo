.PHONY: default
default: run-host-wasmtime

build-guest-rust:
	cargo build --release --target wasm32-wasi --manifest-path ./guest-rust/Cargo.toml

run-host-wasmtime: build-guest-rust
	cargo run --manifest-path ./host-wasmtime/Cargo.toml -- ./target/wasm32-wasi/release/guest-rust.wasm

clean:
	cargo clean
