
# ============================================================
# Docker Bake targets
# ============================================================

build:
	docker buildx bake --load default

push:
	docker buildx bake --push default

test:
	docker buildx bake test

meta-server:
	docker buildx bake --load meta-server-image

web:
	docker buildx bake --load web-image

web-local:
	docker buildx bake web-local

wasm-local:
	docker buildx bake wasm-local

generate-recipe:
	docker buildx bake generate-recipe

web-run: web
	docker run --rm -p 5173:5173 cypherkitty/meta-secret-web:latest npm run dev -- --host 0.0.0.0

taskomatic:
	docker buildx bake --load taskomatic

taskomatic-run: taskomatic
	docker run --rm \
		-v /var/run/docker.sock:/var/run/docker.sock \
		-v $$HOME/.kube:/root/.kube \
		-v $$HOME/.config/k3d:/root/.config/k3d \
		--name taskomatic \
		--workdir /taskomatic \
		localhost/taskomatic:latest

taskomatic-ai:
	docker buildx bake --load taskomatic-ai

sops:
	docker buildx bake --load sops

# ============================================================
# Local dev
# ============================================================

wasm_test:
	cd wasm && wasm-pack test --firefox

wasm_test_headless:
	cd wasm && wasm-pack test --headless --firefox
