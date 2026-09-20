use crate::node::app::sync::environment::{SELECTED_SERVER_ENVIRONMENT, ServerEnvironment};
use run_mode::ClientRunMode;

#[derive(Clone, Debug)]
pub struct ApiUrl {
    url: &'static str,
    port: u32,
    _run_mode: ClientRunMode,
    /// E2E-only endpoint override used by the local network-loss harness.
    /// Normal application builds leave this unset and keep the selected
    /// environment endpoint unchanged.
    endpoint_override: Option<String>,
}

impl ApiUrl {
    pub fn selected() -> Self {
        ApiUrl::for_environment(SELECTED_SERVER_ENVIRONMENT)
    }

    pub fn for_environment(environment: ServerEnvironment) -> Self {
        match environment {
            ServerEnvironment::Local => ApiUrl::local(),
            ServerEnvironment::Remote => ApiUrl::prod(),
        }
    }

    pub fn get(run_mode: ClientRunMode) -> Self {
        match run_mode {
            ClientRunMode::Dev => ApiUrl::dev(),
            ClientRunMode::Prod => ApiUrl::prod(),
        }
    }

    pub fn dev() -> Self {
        ApiUrl {
            url: "https://localhost",
            port: 443,
            _run_mode: ClientRunMode::Dev,
            endpoint_override: None,
        }
    }

    pub fn local() -> Self {
        let endpoint_override = std::env::var("METASECRET_E2E_SERVER_URL")
            .ok()
            .map(|value| value.trim().trim_end_matches('/').to_owned())
            .filter(|value| !value.is_empty());
        ApiUrl {
            url: local_server_url(),
            port: 3000,
            _run_mode: ClientRunMode::Dev,
            endpoint_override,
        }
    }

    pub fn custom_dev(url: &'static str, port: u32) -> Self {
        ApiUrl {
            url,
            port,
            _run_mode: ClientRunMode::Dev,
            endpoint_override: None,
        }
    }

    pub fn prod() -> Self {
        ApiUrl {
            url: "https://api.meta-secret.org",
            port: 443,
            _run_mode: ClientRunMode::Prod,
            endpoint_override: None,
        }
    }
}

#[cfg(target_os = "android")]
fn local_server_url() -> &'static str {
    "http://10.0.2.2"
}

#[cfg(not(target_os = "android"))]
fn local_server_url() -> &'static str {
    "http://127.0.0.1"
}

impl ApiUrl {
    pub fn get_url(&self) -> String {
        self.endpoint_override
            .clone()
            .unwrap_or_else(|| format!("{}:{}", self.url, self.port))
    }
}

pub mod run_mode {
    use anyhow::{Result, bail};
    use wasm_bindgen::prelude::wasm_bindgen;

    pub const DEV: &str = "dev";
    pub const PROD: &str = "prod";

    #[wasm_bindgen]
    #[derive(Copy, Clone, Debug)]
    pub enum ClientRunMode {
        Dev,
        Prod,
    }

    impl ClientRunMode {
        pub fn parse(mode: &str) -> Result<ClientRunMode> {
            match mode {
                DEV => Ok(ClientRunMode::Dev),
                PROD => Ok(ClientRunMode::Prod),
                _ => {
                    bail!("Unknown run mode: {}", mode);
                }
            }
        }
    }
}
