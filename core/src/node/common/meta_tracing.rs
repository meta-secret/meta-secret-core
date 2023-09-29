use tracing::{info_span, Span};

const SERVER_SPAN: &str = "MetaServer";
const CLIENT_SPAN: &str = "MetaClient";
const VD_SPAN: &str = "Vd";

pub fn server_span() -> Span {
    info_span!(SERVER_SPAN)
}

pub fn client_span() -> Span {
    info_span!(CLIENT_SPAN)
}

pub fn vd_span() -> Span {
    info_span!(VD_SPAN)
}
