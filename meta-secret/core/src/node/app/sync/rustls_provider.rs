#[cfg(not(target_arch = "wasm32"))]
use std::sync::Once;

#[cfg(not(target_arch = "wasm32"))]
static RUSTLS_RING_PROVIDER: Once = Once::new();

/// Install the rustls `ring` crypto provider once per process.
/// Required for reqwest + rustls 0.23+ before any HTTPS request; otherwise the client panics with "No provider set".
#[cfg(not(target_arch = "wasm32"))]
pub fn ensure_rustls_ring_crypto_provider() {
    RUSTLS_RING_PROVIDER.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// wasm32 build does not use rustls ring provider initialization.
#[cfg(target_arch = "wasm32")]
pub fn ensure_rustls_ring_crypto_provider() {}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::ensure_rustls_ring_crypto_provider;

    #[test]
    fn ensure_twice_does_not_panic() {
        ensure_rustls_ring_crypto_provider();
        ensure_rustls_ring_crypto_provider();
    }
}
