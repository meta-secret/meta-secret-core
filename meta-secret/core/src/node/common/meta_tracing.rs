use tracing::{Span, info_span};

pub const SERVER_SPAN: &str = "MetaServer";
pub const CLIENT_SPAN: &str = "MetaClient";
pub const VD_SPAN: &str = "Vd";

pub fn server_span() -> Span {
    info_span!(SERVER_SPAN)
}

pub fn client_span() -> Span {
    info_span!(CLIENT_SPAN)
}

pub fn vd_span() -> Span {
    info_span!(VD_SPAN)
}
