# A demo showing usage of `wasi:io/streams`

## Prerequisites

Rust target wasm32-wasi:
```bash
rustup target add wasm32-wasi
```

## Usage

```bash
make
```

## Acknowledgements

- This is based off of [github.com/cpetig/resource-demo](https://github.com/cpetig/resource-demo).
- A lot of heavy lifting is done by [wasi-async-runtime](https://docs.rs/wasi-async-runtime)
