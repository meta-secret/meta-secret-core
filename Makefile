
wasm_test:
	cd wasm && wasm-pack test --firefox

wasm_test_headless:
	cd wasm && wasm-pack test --headless --firefox