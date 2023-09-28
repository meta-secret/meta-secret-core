
clean:
	rm -rf schema/target

build_wasm:
	cd wasm && wasm-pack build --target web

	#rm -rf ui/pkg
	#cp -R wasm/pkg ui

build_js: build_wasm
	cd ui && npm install vue-tsc
	cd ui && npm run build

generate_typescript_models:
	cd ../meta-secret-core/schema && make generate_schema_type_script
	cp -r  ../meta-secret-core/schema/target/core-models-ts/model ui/src

install_wasm_pack:
	curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
	cargo install cargo-generate

wasm_test:
	cd wasm && wasm-pack test --firefox

wasm_test_headless:
	cd wasm && wasm-pack test --headless --firefox