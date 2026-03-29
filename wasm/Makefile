# Build commands for the audex-wasm crate.
# Requires: wasm-pack (https://rustwasm.github.io/wasm-pack/installer/)

.PHONY: build-web build-node test clean check

# Build for browser usage (ES module output in pkg/)
build-web:
	wasm-pack build --target web --release

# Build for Node.js usage (CommonJS output in pkg-node/)
build-node:
	wasm-pack build --target nodejs --release --out-dir pkg-node

# Run browser tests in headless Chrome
test:
	wasm-pack test --headless --chrome

# Run native-target unit tests (no browser required)
test-native:
	cargo test -p audex-wasm

# Type-check for the wasm32 target without producing artifacts
check:
	cargo check -p audex-wasm --target wasm32-unknown-unknown

# Remove generated build output
clean:
	rm -rf pkg/ pkg-node/
