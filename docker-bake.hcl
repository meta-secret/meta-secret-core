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
  contexts = {
    webcli = "meta-secret/web-cli"
  }
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
  contexts = {
    webcli = "meta-secret/web-cli"
  }
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

// Compiles test binaries and pushes registry cache. Run alone before `test` so a
// failing test run does not skip cache export (see Taskfile `test` / CI job steps).
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

target "test" {
  context    = "meta-secret"
  dockerfile = "Dockerfile"
  target     = "test-runner"
  depends_on = ["warm-cache"]
  output     = ["type=cacheonly"]
  cache-from = [
    "type=registry,ref=${REGISTRY}/meta-secret-core:cache",
    "type=registry,ref=${REGISTRY}/meta-secret-server:cache",
  ]
  // Cache export is owned by warm-cache (runs in a separate bake before this target).
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
