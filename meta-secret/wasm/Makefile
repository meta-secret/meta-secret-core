
local_build:
	wasm-pack build --target web
	rm -rf ../web-cli/ui/pkg
	cp -r pkg ../web-cli/ui/
	cp pkg.package.json ../web-cli/ui/pkg/package.json
