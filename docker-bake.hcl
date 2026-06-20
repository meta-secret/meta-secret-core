variable "REGISTRY" {
  default = "ghcr.io/meta-secret/meta-secret-core"
}

// Set PUSH_CACHE=1 to enable writing cache to registry (CI only)
variable "PUSH_CACHE" {
  default = ""
}

// ============================================================
// Groups
// ============================================================

group "default" {
  targets = ["meta-server-image", "web-image"]
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
    "type=registry,ref=${REGISTRY}/meta-secret-core:cache",
  ]
  cache-to = PUSH_CACHE != "" ? ["type=registry,ref=${REGISTRY}/meta-secret-server:cache,mode=max"] : []
}

target "web-image" {
  context    = "meta-secret"
  dockerfile = "Dockerfile"
  target     = "web"
  tags       = ["${REGISTRY}/meta-secret-web:latest"]
  cache-from = [
    "type=registry,ref=${REGISTRY}/meta-secret-web:cache",
    "type=registry,ref=${REGISTRY}/meta-secret-core:cache",
  ]
  cache-to = PUSH_CACHE != "" ? ["type=registry,ref=${REGISTRY}/meta-secret-web:cache,mode=max"] : []
}

target "web-local" {
  context    = "meta-secret"
  dockerfile = "Dockerfile"
  target     = "web-output"
  output     = ["type=local,dest=meta-secret/web-cli/ui/dist"]
  cache-from = [
    "type=registry,ref=${REGISTRY}/meta-secret-web:cache",
    "type=registry,ref=${REGISTRY}/meta-secret-core:cache",
  ]
  cache-to = PUSH_CACHE != "" ? ["type=registry,ref=${REGISTRY}/meta-secret-web:cache,mode=max"] : []
}

// Single bake session: chef-cook wasm deps once, then web-output reuses builder-wasm locally.
// Registry cache alone is unreliable across separate GHA runners/jobs.
group "web-preview" {
  targets = ["warm-cache-wasm", "web-local"]
}

target "wasm-local" {
  context    = "meta-secret"
  dockerfile = "Dockerfile"
  target     = "wasm-output"
  output     = ["type=local,dest=meta-secret/web-cli/ui/pkg"]
  cache-from = [
    "type=registry,ref=${REGISTRY}/meta-secret-web:cache",
    "type=registry,ref=${REGISTRY}/meta-secret-core:cache",
  ]
  cache-to = PUSH_CACHE != "" ? ["type=registry,ref=${REGISTRY}/meta-secret-web:cache,mode=max"] : []
}

// Same-session reuse as web-preview (chef cook once, then wasm-output).
group "wasm-pkg" {
  targets = ["warm-cache-wasm", "wasm-local"]
}

// Warms host test-compiler dep layers into registry cache (CI test job).
// Run before the test target so compilation is always cached even if tests fail.
target "warm-cache" {
  context    = "meta-secret"
  dockerfile = "Dockerfile"
  target     = "test-compiler"
  output     = ["type=cacheonly"]
  cache-from = [
    "type=registry,ref=${REGISTRY}/meta-secret-core:cache",
    "type=registry,ref=${REGISTRY}/meta-secret-server:cache",
  ]
  cache-to = PUSH_CACHE != "" ? ["type=registry,ref=${REGISTRY}/meta-secret-core:cache,mode=max"] : []
}

// Single bake session: host chef cook + test-runner reuses test-compiler locally.
group "test-ci" {
  targets = ["warm-cache", "test"]
}

target "test" {
  context    = "meta-secret"
  dockerfile = "Dockerfile"
  target     = "test-runner"
  output     = ["type=cacheonly"]
  cache-from = [
    "type=registry,ref=${REGISTRY}/meta-secret-core:cache",
    "type=registry,ref=${REGISTRY}/meta-secret-server:cache",
  ]
  cache-to = []
}

// Warms the wasm32 dep cache without doing a full web build.
// Run once to populate meta-secret-web:cache with wasm deps.
target "warm-cache-wasm" {
  context    = "meta-secret"
  dockerfile = "Dockerfile"
  target     = "builder-wasm"
  output     = ["type=cacheonly"]
  cache-from = [
    "type=registry,ref=${REGISTRY}/meta-secret-web:cache",
    "type=registry,ref=${REGISTRY}/meta-secret-core:cache",
  ]
  cache-to = PUSH_CACHE != "" ? ["type=registry,ref=${REGISTRY}/meta-secret-web:cache,mode=max"] : []
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
