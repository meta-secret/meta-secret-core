pub mod info;
pub use info::get_app_info;

include!(concat!(env!("OUT_DIR"), "/meta_core.uniffi.rs"));


