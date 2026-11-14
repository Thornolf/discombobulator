use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use discombobulator_core::library::{ProgressSnapshot, Variant};
use discombobulator_core::model::{Decision, Photo};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::folders;
use crate::session::Session;

pub const LAST_FOLDER: &str = "last_folder";

#[derive(Debug)]
pub struct ApiError(StatusCode, String);

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> ApiError {
        ApiError(status, message.into())
    }

    fn internal(error: impl std::fmt::Display) -> ApiError {
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

pub fn session(state: &AppState) -> ApiResult<Arc<Session>> {
    state
        .session
        .read()
        .map_err(ApiError::internal)?
        .clone()
        .ok_or_else(|| ApiError::new(StatusCode::CONFLICT, "No folder is open"))
}

#[derive(Serialize)]
pub struct StateDto {
    root: Option<String>,
    version: u64,
    processing: bool,
    progress: Option<ProgressSnapshot>,
    unreadable: usize,
    can_open_folders: bool,
    decisions_seq: i64,
}

pub const PHONE_CANNOT_OPEN: &str =
    "Folders can only be browsed and opened on the PC. The phone uses the folder the PC has open.";

fn refuse_tailnet(headers: &HeaderMap) -> ApiResult<()> {
    if crate::guard::came_through_tailnet(headers) {
        return Err(ApiError::new(StatusCode::FORBIDDEN, PHONE_CANNOT_OPEN));
    }
    Ok(())
}

pub async fn state(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<StateDto>> {
    let current = state.session.read().map_err(ApiError::internal)?.clone();
    Ok(Json(StateDto {
        can_open_folders: !crate::guard::came_through_tailnet(&headers),
        decisions_seq: state.store.decisions_seq(),
        root: current
            .as_ref()
            .map(|session| session.library.root().to_string_lossy().into_owned()),
        version: state.version.load(Ordering::Relaxed),
        processing: current
            .as_ref()
            .is_some_and(|session| session.is_processing()),
        progress: current.as_ref().map(|session| session.library.progress()),
        unreadable: current
            .as_ref()
            .map_or(0, |session| session.library.last_scan_failures().count),
    }))
}

#[derive(Deserialize)]
pub struct FolderQuery {
    path: Option<String>,
}

pub async fn folders(
    headers: HeaderMap,
    Query(query): Query<FolderQuery>,
) -> ApiResult<Json<folders::Listing>> {
    refuse_tailnet(&headers)?;
    match query.path.filter(|path| !path.is_empty()) {
        None => Ok(Json(folders::Listing {
            path: None,
            parent: None,
            entries: folders::roots(),
        })),
        Some(path) => tokio::task::spawn_blocking(move || folders::list(&PathBuf::from(path)))
            .await
            .map_err(ApiError::internal)?
            .map(Json)
            .map_err(|error| ApiError::new(StatusCode::NOT_FOUND, error.to_string())),
    }
}

#[derive(Deserialize)]
pub struct OpenRequest {
    path: String,
}

pub fn open_folder(state: &AppState, path: PathBuf) -> ApiResult<()> {
    let session = Session::open(
        &path,
        Arc::clone(&state.store),
        Arc::clone(&state.cache),
        Arc::clone(&state.version),
    )
    .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    let root = session.library.root().to_string_lossy().into_owned();
    *state.session.write().map_err(ApiError::internal)? = Some(Arc::new(session));
    state
        .store
        .set_setting(LAST_FOLDER, &root)
        .map_err(ApiError::internal)?;
    Ok(())
}

pub async fn open(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<OpenRequest>,
) -> ApiResult<Json<StateDto>> {
    refuse_tailnet(&headers)?;
    let path = PathBuf::from(request.path);
    let opener = Arc::clone(&state);
    tokio::task::spawn_blocking(move || -> ApiResult<()> {
        let runtime = tokio::runtime::Handle::current();
        let _guard = runtime.enter();
        open_folder(&opener, path)
    })
    .await
    .map_err(ApiError::internal)??;
    self::state(State(state), headers).await
}

#[derive(Serialize)]
pub struct PhotoDto {
    id: String,
    name: String,
    folder: String,
    time_ms: i64,
    captured: bool,
    dhash: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    raf: bool,
    jpg: bool,
    size: u64,
    error: Option<String>,
    version: String,
    camera_rating: Option<u8>,
    decision: Decision,
}

fn photo_dto(photo: &Photo, root: &std::path::Path) -> PhotoDto {
    PhotoDto {
        id: photo.id.clone(),
        name: photo.name(),
        folder: photo
            .dir
            .strip_prefix(root)
            .map(|relative| relative.to_string_lossy().into_owned())
            .unwrap_or_default(),
        time_ms: photo.time_ms(),
        captured: photo.capture_ms.is_some(),
        dhash: photo.dhash.map(|hash| format!("{hash:016x}")),
        width: photo.width,
        height: photo.height,
        raf: photo.raf.is_some(),
        jpg: photo.jpg.is_some(),
        size: photo.size,
        error: photo.error.clone(),
        version: photo.fingerprint.clone(),
        camera_rating: photo.camera_rating,
        decision: photo.effective_decision(),
    }
}

pub async fn photos(State(state): State<Arc<AppState>>) -> ApiResult<Json<Vec<PhotoDto>>> {
    let session = session(&state)?;
    tokio::task::spawn_blocking(move || {
        let library = &session.library;
        let photos = library.photos().map_err(ApiError::internal)?;
        Ok(Json(
            photos
                .iter()
                .map(|photo| photo_dto(photo, library.root()))
                .collect(),
        ))
    })
    .await
    .map_err(ApiError::internal)?
}

fn image_response(bytes: Vec<u8>) -> Response {
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    response
}

fn valid_id(id: &str) -> bool {
    discombobulator_core::library::is_photo_id(id)
}

async fn image(state: Arc<AppState>, id: String, variant: Variant) -> ApiResult<Response> {
    if !valid_id(&id) {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "Invalid photo id"));
    }
    let session = session(&state)?;
    let bytes = tokio::task::spawn_blocking(move || session.library.image(&id, variant))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;
    bytes
        .map(image_response)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "No image for this photo"))
}

pub async fn thumb(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    image(state, id, Variant::Thumb).await
}

pub async fn preview(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    image(state, id, Variant::Preview).await
}

#[derive(Deserialize)]
pub struct DecisionChange {
    id: String,
    decision: Decision,
}

#[derive(Deserialize)]
pub struct DecisionBatch {
    changes: Vec<DecisionChange>,
}

#[derive(Serialize)]
pub struct BatchResult {
    applied: usize,
    missing: Vec<String>,
}

pub const MAX_BATCH: usize = 100_000;

pub fn apply_decisions(state: &AppState, changes: &[DecisionChange]) -> ApiResult<BatchResult> {
    if changes.len() > MAX_BATCH {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Too many changes",
        ));
    }
    if changes.iter().any(|change| change.decision.rating > 5) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Rating must be 0 to 5",
        ));
    }
    let session = session(state)?;
    let now = now_ms();
    let mut result = BatchResult {
        applied: 0,
        missing: Vec::new(),
    };
    for change in changes {
        let known = valid_id(&change.id)
            && session
                .library
                .photo(&change.id)
                .map_err(ApiError::internal)?
                .is_some();
        if !known {
            result.missing.push(change.id.clone());
            continue;
        }
        state
            .store
            .set_decision(&change.id, change.decision, now)
            .map_err(ApiError::internal)?;
        result.applied += 1;
    }
    Ok(result)
}

pub async fn decisions(
    State(state): State<Arc<AppState>>,
    Json(batch): Json<DecisionBatch>,
) -> ApiResult<Json<BatchResult>> {
    let worker = Arc::clone(&state);
    tokio::task::spawn_blocking(move || apply_decisions(&worker, &batch.changes))
        .await
        .map_err(ApiError::internal)?
        .map(Json)
}

#[derive(Deserialize)]
pub struct SinceQuery {
    since: Option<i64>,
}

#[derive(Serialize)]
pub struct DecisionChangeDto {
    id: String,
    decision: Decision,
}

#[derive(Serialize)]
pub struct DecisionChangesDto {
    seq: i64,
    changes: Vec<DecisionChangeDto>,
}

pub async fn decisions_since(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SinceQuery>,
) -> ApiResult<Json<DecisionChangesDto>> {
    let since = query.since.unwrap_or(0).max(0);
    tokio::task::spawn_blocking(move || {
        let found = state
            .store
            .decisions_since(since)
            .map_err(ApiError::internal)?;
        Ok(Json(DecisionChangesDto {
            seq: found.seq,
            changes: found
                .changes
                .into_iter()
                .map(|(id, decision)| DecisionChangeDto { id, decision })
                .collect(),
        }))
    })
    .await
    .map_err(ApiError::internal)?
}

pub async fn apply_plan(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<discombobulator_core::apply::Plan>> {
    let session = session(&state)?;
    tokio::task::spawn_blocking(move || discombobulator_core::apply::plan(&session.library))
        .await
        .map_err(ApiError::internal)?
        .map(Json)
        .map_err(ApiError::internal)
}

pub async fn apply(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<discombobulator_core::apply::Report>> {
    let session = session(&state)?;
    let version = Arc::clone(&state.version);
    let report = tokio::task::spawn_blocking(move || {
        let _guard = state
            .apply_lock
            .lock()
            .map_err(|_| ApiError::internal("apply lock poisoned"))?;
        discombobulator_core::apply::apply(&session.library).map_err(ApiError::internal)
    })
    .await
    .map_err(ApiError::internal)??;
    version.fetch_add(1, Ordering::Relaxed);
    Ok(Json(report))
}

#[derive(Deserialize)]
pub struct DuplicateQuery {
    tolerance: Option<u32>,
}

pub async fn duplicates(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DuplicateQuery>,
) -> ApiResult<Json<Vec<discombobulator_core::duplicates::Group>>> {
    let session = session(&state)?;
    let tolerance = query
        .tolerance
        .unwrap_or(6)
        .min(discombobulator_core::duplicates::MAX_TOLERANCE);
    tokio::task::spawn_blocking(move || {
        let mut cache = state
            .exact_duplicates
            .lock()
            .map_err(|_| ApiError::internal("duplicates lock poisoned"))?;
        let library = &session.library;
        let version = state.version.load(Ordering::Relaxed);
        let fresh = cache
            .as_ref()
            .is_some_and(|entry| entry.version == version && entry.root == library.root());
        if !fresh {
            let photos = library.photos().map_err(ApiError::internal)?;
            *cache = Some(crate::ExactCache {
                root: library.root().to_path_buf(),
                version,
                groups: discombobulator_core::duplicates::exact_groups(library, &photos),
            });
        }
        let exact = cache
            .as_ref()
            .map(|entry| entry.groups.clone())
            .unwrap_or_default();
        discombobulator_core::duplicates::combine(library, exact, tolerance)
            .map_err(ApiError::internal)
    })
    .await
    .map_err(ApiError::internal)?
    .map(Json)
}

pub async fn phone_status(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<crate::phone::PhoneStatus>> {
    let worker = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let status = crate::phone::status(worker.port);
        worker.remember_tailnet_host(status.machine.clone());
        status
    })
    .await
    .map(Json)
    .map_err(ApiError::internal)
}

pub async fn phone_enable(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<crate::phone::PhoneStatus>> {
    let worker = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let status = crate::phone::enable(worker.port);
        worker.remember_tailnet_host(status.machine.clone());
        status
    })
    .await
    .map(Json)
    .map_err(ApiError::internal)
}

pub async fn recycle_plan(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<discombobulator_core::recycle::RecyclePlan>> {
    let session = session(&state)?;
    tokio::task::spawn_blocking(move || discombobulator_core::recycle::plan(session.library.root()))
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

pub async fn recycle(
    State(state): State<Arc<AppState>>,
    Json(confirmed): Json<discombobulator_core::recycle::RecyclePlan>,
) -> ApiResult<Json<discombobulator_core::recycle::RecycleReport>> {
    let session = session(&state)?;
    tokio::task::spawn_blocking(move || {
        let _guard = state
            .apply_lock
            .lock()
            .map_err(|_| ApiError::internal("apply lock poisoned"))?;
        let root = session.library.root();
        if discombobulator_core::recycle::plan(root) != confirmed {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "The _to_delete folders changed. Check the new count and confirm again.",
            ));
        }
        Ok(discombobulator_core::recycle::recycle(root))
    })
    .await
    .map_err(ApiError::internal)?
    .map(Json)
}
