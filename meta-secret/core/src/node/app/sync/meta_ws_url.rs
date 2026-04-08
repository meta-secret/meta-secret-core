//! Build WebSocket URL for `/meta_ws` from the same HTTP API base used for `/meta_request`.

/// Converts `http(s)://host:port` to `ws(s)://host:port/meta_ws`.
pub fn meta_ws_url_from_http_api_base(api_base: &str) -> String {
    let trimmed = api_base.trim_end_matches('/');
    if trimmed.starts_with("https://") {
        format!("{}/meta_ws", trimmed.replacen("https://", "wss://", 1))
    } else if trimmed.starts_with("http://") {
        format!("{}/meta_ws", trimmed.replacen("http://", "ws://", 1))
    } else {
        format!("ws://{}/meta_ws", trimmed.trim_start_matches('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_to_wss() {
        assert_eq!(
            meta_ws_url_from_http_api_base("https://api.example.com:443"),
            "wss://api.example.com:443/meta_ws"
        );
    }

    #[test]
    fn http_to_ws_trims_slash() {
        assert_eq!(
            meta_ws_url_from_http_api_base("http://127.0.0.1:3000/"),
            "ws://127.0.0.1:3000/meta_ws"
        );
    }

    #[test]
    fn bare_host_gets_ws_prefix() {
        assert_eq!(
            meta_ws_url_from_http_api_base("127.0.0.1:3000"),
            "ws://127.0.0.1:3000/meta_ws"
        );
    }

    #[test]
    fn bare_host_leading_slash_trimmed() {
        assert_eq!(
            meta_ws_url_from_http_api_base("/127.0.0.1:3000"),
            "ws://127.0.0.1:3000/meta_ws"
        );
    }
}
