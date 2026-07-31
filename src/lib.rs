pub mod api;
pub mod config;
pub mod error;
pub mod spotify;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/com.spotify.canvazcache.rs"));
}
