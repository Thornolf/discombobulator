pub mod imaging;
pub mod library;
pub mod metadata;
pub mod model;
pub mod raf;
pub mod scan;
pub mod store;
pub mod xmp;

use std::path::PathBuf;

pub const APP_NAME: &str = "discombobulator";

pub fn data_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("com", "thornolf", APP_NAME)
        .map(|dirs| dirs.data_local_dir().to_path_buf())
}
