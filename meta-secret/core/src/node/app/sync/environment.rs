#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ServerEnvironment {
    Local,
    Remote,
}

pub const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::Remote;

pub fn parse_server_environment(environment: &str) -> ServerEnvironment {
    match environment.trim().to_lowercase().as_str() {
        "local" => ServerEnvironment::Local,
        "remote" | "prod" | "production" => ServerEnvironment::Remote,
        _ => SELECTED_SERVER_ENVIRONMENT,
    }
}
