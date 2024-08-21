
clean:
	rm -rf ui/src/model
	rm -rf ui/node_modules
	rm -rf ui/dist
	rm -rf ui/package-lock.json
	rm -rf ../meta-secret-core/schema/target

build_wasm:
	cd ~/coding/github/meta-secret-core/wasm && wasm-pack build --debug --target web

	rm -rf ui/pkg
	cp -R ~/coding/github/meta-secret-core/wasm/pkg ui

build_js: build_wasm
	cd ui && npm install vue-tsc
	cd ui && npm run build

run: build_wasm
	cd ui && npm run dev

generate_typescript_models:
	cd ~/coding/github/meta-secret-core/schema && make generate_schema_type_script
	cp -r  ~/coding/github/meta-secret-core/schema/target/core-models-ts/model ui/src

install_wasm_pack:
	curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
	cargo install cargo-generate
