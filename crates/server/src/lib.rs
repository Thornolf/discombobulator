pub mod api;
pub mod assets;
pub mod folders;
pub mod guard;
pub mod phone;
pub mod session;

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock};

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use discombobulator_core::library::Cache;
use discombobulator_core::store::Store;

pub const DEFAULT_PORT: u16 = 47820;
const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

pub struct AppState {
    pub store: Arc<Store>,
    pub cache: Arc<Cache>,
    pub session: RwLock<Option<Arc<session::Session>>>,
    pub version: Arc<AtomicU64>,
    pub apply_lock: std::sync::Mutex<()>,
    pub exact_duplicates: std::sync::Mutex<Option<ExactCache>>,
    pub port: u16,
    pub tailnet_host: RwLock<Option<String>>,
}

pub struct ExactCache {
    pub root: PathBuf,
    pub version: u64,
    pub groups: Vec<Vec<String>>,
}

impl AppState {
    pub fn open(
        data_dir: &std::path::Path,
        port: u16,
    ) -> Result<Arc<AppState>, Box<dyn std::error::Error + Send + Sync>> {
        discombobulator_core::fsx::create_private_dir(data_dir)?;
        Ok(Arc::new(AppState {
            store: Arc::new(Store::open(&data_dir.join("library.sqlite3"))?),
            cache: Arc::new(Cache::new(data_dir.join("cache"))?),
            session: RwLock::new(None),
            version: Arc::new(AtomicU64::new(1)),
            apply_lock: std::sync::Mutex::new(()),
            exact_duplicates: std::sync::Mutex::new(None),
            port,
            tailnet_host: RwLock::new(None),
        }))
    }

    pub fn refresh_tailnet_host(&self) {
        let name = phone::machine_name();
        if let Ok(mut current) = self.tailnet_host.write() {
            *current = name;
        }
    }

    pub fn reopen_last_folder(&self) {
        if let Ok(Some(path)) = self.store.setting(api::LAST_FOLDER) {
            let _ = api::open_folder(self, PathBuf::from(path));
        }
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/state", get(api::state))
        .route("/api/folders", get(api::folders))
        .route("/api/library", post(api::open))
        .route("/api/photos", get(api::photos))
        .route("/api/photos/{id}/thumb", get(api::thumb))
        .route("/api/photos/{id}/preview", get(api::preview))
        .route("/api/decisions", post(api::decisions))
        .route("/api/apply", get(api::apply_plan).post(api::apply))
        .route("/api/duplicates", get(api::duplicates))
        .route("/api/phone", get(api::phone_status).post(api::phone_enable))
        .fallback(assets::serve)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            guard::middleware,
        ))
        .with_state(state)
}

pub fn local_address(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

pub async fn serve(state: Arc<AppState>, listener: tokio::net::TcpListener) -> std::io::Result<()> {
    axum::serve(listener, router(state)).await
}

pub fn default_data_dir() -> Option<PathBuf> {
    discombobulator_core::data_dir()
}
