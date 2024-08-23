.PHONY: default
default: run-host-wasmtime

download-reactor:
	[ ! -f wasi_snapshot_preview1.reactor.wasm ] && \
	   curl -L -O https://github.com/bytecodealliance/wasmtime/releases/download/dev/wasi_snapshot_preview1.reactor.wasm \
	|| echo "reactor already downloaded"

build-guest-rust: download-reactor
	cargo build --release --target wasm32-wasi --manifest-path ./guest-rust/Cargo.toml
	wasm-tools component new ./target/wasm32-wasi/release/guest-rust.wasm -o component.wasm --adapt wasi_snapshot_preview1.reactor.wasm

run-host-wasmtime: build-guest-rust
	cargo run --manifest-path ./host-wasmtime/Cargo.toml

clean:
	rm -f wasi_snapshot_preview1.reactor.wasm
	rm -f component.reactor.wasm
	cargo clean
