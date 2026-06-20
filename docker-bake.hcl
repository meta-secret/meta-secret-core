variable "REGISTRY" {
  default = "ghcr.io/meta-secret/meta-secret-core"
}

// Set PUSH_CACHE=1 in CI to push deps images (builder-*:cache tags).
variable "PUSH_CACHE" {
  default = ""
}

// ============================================================
// Groups
// ============================================================

group "default" {
  targets = ["meta-server-image", "web-image"]
}

// Push builder-debug:cache, then compile/run tests (same bake, --push in CI).
group "test-ci" {
  targets = ["builder-debug-cache", "test"]
}

group "web-preview" {
  targets = ["builder-wasm-cache", "web-local"]
}

group "wasm-pkg" {
  targets = ["builder-wasm-cache", "wasm-local"]
}

// ============================================================
// Meta-Secret builds (meta-secret/Dockerfile)
// ============================================================

target "meta-server-image" {
  context    = "meta-secret"
  dockerfile = "Dockerfile"
  target     = "meta-server"
  tags       = ["${REGISTRY}/meta-secret-server:latest"]
  cache-from = [
    "type=registry,ref=${REGISTRY}/meta-secret-server:cache",
    "type=registry,ref=${REGISTRY}/builder-debug:cache",
  ]
  cache-to = PUSH_CACHE != "" ? ["type=registry,ref=${REGISTRY}/meta-secret-server:cache,mode=max"] : []
}

target "web-image" {
  context    = "meta-secret"
  dockerfile = "Dockerfile"
  target     = "web"
  contexts = {
    webcli = "meta-secret/web-cli"
  }
  tags       = ["${REGISTRY}/meta-secret-web:latest"]
  cache-from = [
    "type=registry,ref=${REGISTRY}/meta-secret-web:cache",
    "type=registry,ref=${REGISTRY}/builder-wasm:cache",
  ]
  cache-to = PUSH_CACHE != "" ? ["type=registry,ref=${REGISTRY}/meta-secret-web:cache,mode=max"] : []
}

target "web-local" {
  context    = "meta-secret"
  dockerfile = "Dockerfile"
  target     = "web-output"
  contexts = {
    webcli = "meta-secret/web-cli"
  }
  depends_on = ["builder-wasm-cache"]
  output     = ["type=local,dest=meta-secret/web-cli/ui/dist"]
  cache-from = PUSH_CACHE != "" ? [
    "type=gha,scope=meta-secret-wasm",
    "type=registry,ref=${REGISTRY}/builder-wasm:cache",
  ] : [
    "type=registry,ref=${REGISTRY}/builder-wasm:cache",
  ]
}

target "wasm-local" {
  context    = "meta-secret"
  dockerfile = "Dockerfile"
  target     = "wasm-output"
  depends_on = ["builder-wasm-cache"]
  output     = ["type=local,dest=meta-secret/web-cli/ui/pkg"]
  cache-from = PUSH_CACHE != "" ? [
    "type=gha,scope=meta-secret-wasm",
    "type=registry,ref=${REGISTRY}/builder-wasm:cache",
  ] : [
    "type=registry,ref=${REGISTRY}/builder-wasm:cache",
  ]
}

// Pre-compiled debug deps (cargo chef cook --tests). Pushed as a rolling :cache tag in CI.
target "builder-debug-cache" {
  context    = "meta-secret"
  dockerfile = "Dockerfile"
  target     = "builder-debug"
  tags       = ["${REGISTRY}/builder-debug:cache"]
  output     = PUSH_CACHE != "" ? ["type=registry"] : ["type=cacheonly"]
  cache-from = PUSH_CACHE != "" ? [
    "type=gha,scope=meta-secret-test",
    "type=registry,ref=${REGISTRY}/builder-debug:cache",
  ] : [
    "type=registry,ref=${REGISTRY}/builder-debug:cache",
  ]
}

target "test" {
  context    = "meta-secret"
  dockerfile = "Dockerfile"
  target     = "test-runner"
  depends_on = ["builder-debug-cache"]
  output     = ["type=cacheonly"]
  cache-from = PUSH_CACHE != "" ? [
    "type=gha,scope=meta-secret-test",
    "type=registry,ref=${REGISTRY}/builder-debug:cache",
  ] : [
    "type=registry,ref=${REGISTRY}/builder-debug:cache",
  ]
}

// Pre-compiled wasm32 deps (cargo chef cook). Pushed as a rolling :cache tag in CI.
target "builder-wasm-cache" {
  context    = "meta-secret"
  dockerfile = "Dockerfile"
  target     = "builder-wasm"
  tags       = ["${REGISTRY}/builder-wasm:cache"]
  output     = PUSH_CACHE != "" ? ["type=registry"] : ["type=cacheonly"]
  cache-from = PUSH_CACHE != "" ? [
    "type=gha,scope=meta-secret-wasm",
    "type=registry,ref=${REGISTRY}/builder-wasm:cache",
  ] : [
    "type=registry,ref=${REGISTRY}/builder-wasm:cache",
  ]
}

target "generate-recipe" {
  context    = "meta-secret"
  dockerfile = "Dockerfile"
  target     = "recipe-output"
  output     = ["type=local,dest=meta-secret"]
}

target "playwright" {
  context    = "meta-secret"
  dockerfile = "Dockerfile"
  target     = "playwright"
  contexts = {
    webcli = "meta-secret/web-cli"
  }
  tags       = ["${REGISTRY}/playwright:latest"]
  cache-from = [
    "type=registry,ref=${REGISTRY}/playwright:cache",
  ]
  cache-to = PUSH_CACHE != "" ? ["type=registry,ref=${REGISTRY}/playwright:cache,mode=max"] : []
}

// ============================================================
// Infra builds
// ============================================================

target "taskomatic" {
  context    = "infra"
  dockerfile = "Dockerfile.taskomatic"
  target     = "taskomatic"
  tags       = ["localhost/taskomatic:latest"]
}

target "sops" {
  context    = "infra"
  dockerfile = "Dockerfile.sops"
  target     = "sops"
  tags       = ["localhost/sops:latest"]
}
