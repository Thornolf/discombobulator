pub mod apply;
pub mod fsx;
pub mod imaging;
pub mod library;
pub mod metadata;
pub mod model;
pub mod raf;
pub mod scan;
pub mod sidecar;
pub mod store;
pub mod xmp;

use std::path::PathBuf;

pub const APP_NAME: &str = "discombobulator";

pub fn data_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.data_local_dir().join(APP_NAME))
}
