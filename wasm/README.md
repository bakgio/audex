# audex-wasm

WebAssembly bindings for the `audex` audio metadata library.

## Build

- Browser package: `wasm-pack build --target web --release`
- Node package: `wasm-pack build --target nodejs --release --out-dir pkg-node`

## Test

- Node: `wasm-pack test --node --test web`

## License

Licensed under either of these licenses, at your option:

- Apache-2.0
- MIT
