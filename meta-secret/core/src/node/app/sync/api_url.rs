use run_mode::ClientRunMode;

#[derive(Copy, Clone, Debug)]
pub struct ApiUrl {
    url: &'static str,
    port: u32,
    _run_mode: ClientRunMode,
}

impl ApiUrl {
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
        }
    }

    pub fn prod() -> Self {
        ApiUrl {
            url: "https://api.meta-secret.org",
            port: 443,
            _run_mode: ClientRunMode::Prod,
        }
    }
}

impl ApiUrl {
    pub fn get_url(&self) -> String {
        format!("{}:{}", self.url, self.port)
    }
}

pub mod run_mode {
    use anyhow::{bail, Result};
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
