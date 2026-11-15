use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use discombobulator_server::{AppState, router};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Fixture {
    _dir: tempfile::TempDir,
    photos: std::path::PathBuf,
    state: Arc<AppState>,
}

fn jpeg() -> Vec<u8> {
    let image = image::RgbImage::from_fn(64, 48, |x, y| {
        image::Rgb([(x * 4) as u8, (y * 5) as u8, 90])
    });
    discombobulator_core::imaging::encode(&image).unwrap_or_default()
}

fn fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let photos = dir.path().join("photos");
    std::fs::create_dir_all(&photos)?;
    std::fs::write(photos.join("DSCF0001.JPG"), jpeg())?;
    std::fs::write(photos.join("DSCF0002.JPG"), jpeg())?;
    let state = AppState::open(&dir.path().join("data"), 47820).map_err(|e| e.to_string())?;
    Ok(Fixture {
        _dir: dir,
        photos,
        state,
    })
}

fn request(
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> Result<Request<Body>, axum::http::Error> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "127.0.0.1:47820")
        .header("x-discombobulator", "1")
        .header("content-type", "application/json");
    builder.body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
}

async fn send(
    state: &Arc<AppState>,
    request: Request<Body>,
) -> Result<(StatusCode, Value), Box<dyn std::error::Error>> {
    let response = router(Arc::clone(state)).oneshot(request).await?;
    let status = response.status();
    let bytes = response.into_body().collect().await?.to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Ok((status, value))
}

async fn open_and_wait(fixture: &Fixture) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let path = fixture.photos.to_string_lossy().into_owned();
    let (status, _) = send(
        &fixture.state,
        request("POST", "/api/library", Some(json!({ "path": path })))?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    for _ in 0..100 {
        let (_, state) = send(&fixture.state, request("GET", "/api/state", None)?).await?;
        if state["processing"] == json!(false) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (_, photos) = send(&fixture.state, request("GET", "/api/photos", None)?).await?;
    Ok(photos.as_array().cloned().unwrap_or_default())
}

#[tokio::test(flavor = "multi_thread")]
async fn opens_folder_lists_photos_and_serves_thumbs() -> TestResult {
    let fixture = fixture()?;
    let photos = open_and_wait(&fixture).await?;
    assert_eq!(photos.len(), 2);
    assert_eq!(photos[0]["name"], json!("DSCF0001.JPG"));
    let (_, state) = send(&fixture.state, request("GET", "/api/state", None)?).await?;
    assert_eq!(state["unreadable"], json!(0));
    let id = photos[0]["id"].as_str().unwrap_or_default().to_owned();
    let response = router(Arc::clone(&fixture.state))
        .oneshot(request("GET", &format!("/api/photos/{id}/thumb"), None)?)
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "image/jpeg");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn serves_shot_info_for_known_photos_only() -> TestResult {
    let fixture = fixture()?;
    let photos = open_and_wait(&fixture).await?;
    let id = photos[0]["id"].as_str().unwrap_or_default().to_owned();
    let (status, info) = send(
        &fixture.state,
        request("GET", &format!("/api/photos/{id}/info"), None)?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(info["faces"], json!([]));
    assert_eq!(info["focus_point"], Value::Null);
    let unknown = "0".repeat(id.len());
    let (status, _) = send(
        &fixture.state,
        request("GET", &format!("/api/photos/{unknown}/info"), None)?,
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(
        &fixture.state,
        request("GET", "/api/photos/..%2Fsecret/info", None)?,
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn decisions_are_saved_and_unknown_ids_reported() -> TestResult {
    let fixture = fixture()?;
    let photos = open_and_wait(&fixture).await?;
    let id = photos[1]["id"].as_str().unwrap_or_default().to_owned();
    let body = json!({ "changes": [
        { "id": id, "decision": { "mark": "delete", "favorite": true, "rating": 3 } },
        { "id": "abcdef", "decision": { "mark": null, "favorite": false, "rating": 0 } }
    ]});
    let (status, result) = send(
        &fixture.state,
        request("POST", "/api/decisions", Some(body))?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["applied"], json!(1));
    assert_eq!(result["missing"], json!(["abcdef"]));
    let (_, photos) = send(&fixture.state, request("GET", "/api/photos", None)?).await?;
    assert_eq!(
        photos[1]["decision"],
        json!({ "mark": "delete", "favorite": true, "rating": 3 })
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn decision_changes_are_listed_by_sequence() -> TestResult {
    let fixture = fixture()?;
    let photos = open_and_wait(&fixture).await?;
    let first = photos[0]["id"].as_str().unwrap_or_default().to_owned();
    let second = photos[1]["id"].as_str().unwrap_or_default().to_owned();
    let (_, state) = send(&fixture.state, request("GET", "/api/state", None)?).await?;
    let start = state["decisions_seq"].as_i64().unwrap_or(-1);
    assert_eq!(start, 0);
    let body = json!({ "changes": [
        { "id": first, "decision": { "mark": "approved", "favorite": false, "rating": 0 } },
        { "id": second, "decision": { "mark": "delete", "favorite": false, "rating": 2 } }
    ]});
    send(
        &fixture.state,
        request("POST", "/api/decisions", Some(body))?,
    )
    .await?;
    let (_, state) = send(&fixture.state, request("GET", "/api/state", None)?).await?;
    assert_eq!(state["decisions_seq"], json!(2));
    let (status, all) = send(
        &fixture.state,
        request("GET", "/api/decisions?since=0", None)?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all["seq"], json!(2));
    assert_eq!(all["changes"].as_array().map(Vec::len), Some(2));
    let body = json!({ "changes": [
        { "id": first, "decision": { "mark": null, "favorite": true, "rating": 0 } }
    ]});
    send(
        &fixture.state,
        request("POST", "/api/decisions", Some(body))?,
    )
    .await?;
    let (_, later) = send(
        &fixture.state,
        request("GET", "/api/decisions?since=2", None)?,
    )
    .await?;
    assert_eq!(later["seq"], json!(3));
    assert_eq!(
        later["changes"],
        json!([{ "id": first, "decision": { "mark": null, "favorite": true, "rating": 0 } }])
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_bad_rating_and_bad_ids() -> TestResult {
    let fixture = fixture()?;
    let photos = open_and_wait(&fixture).await?;
    let id = photos[0]["id"].as_str().unwrap_or_default().to_owned();
    let body = json!({ "changes": [{ "id": id, "decision": { "mark": null, "favorite": false, "rating": 9 } }] });
    let (status, _) = send(
        &fixture.state,
        request("POST", "/api/decisions", Some(body))?,
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = send(
        &fixture.state,
        request("GET", "/api/photos/..%2F..%2Fetc/thumb", None)?,
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn cross_site_writes_and_foreign_hosts_are_refused() -> TestResult {
    let fixture = fixture()?;
    let no_header = Request::builder()
        .method("POST")
        .uri("/api/library")
        .header("host", "127.0.0.1:47820")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "path": "/" }).to_string()))?;
    let (status, _) = send(&fixture.state, no_header).await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let foreign = Request::builder()
        .uri("/api/state")
        .header("host", "evil.example")
        .body(Body::empty())?;
    let (status, _) = send(&fixture.state, foreign).await?;
    assert_eq!(status, StatusCode::MISDIRECTED_REQUEST);
    let cross_site_get = Request::builder()
        .uri("/api/folders?path=%5C%5Cattacker.example%5Cshare")
        .header("host", "127.0.0.1:47820")
        .body(Body::empty())?;
    let (status, _) = send(&fixture.state, cross_site_get).await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    Ok(())
}

fn tailnet_request(
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> Result<Request<Body>, axum::http::Error> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "127.0.0.1:47820")
        .header("x-forwarded-host", "desk.tail1234.ts.net")
        .header("tailscale-user-login", "me@example.com")
        .header("x-discombobulator", "1")
        .header("content-type", "application/json")
        .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
}

#[tokio::test(flavor = "multi_thread")]
async fn tailnet_clients_cannot_browse_or_open_folders() -> TestResult {
    let fixture = fixture()?;
    let path = fixture.photos.to_string_lossy().into_owned();
    let (status, body) = send(
        &fixture.state,
        tailnet_request("GET", "/api/folders", None)?,
    )
    .await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|text| text.contains("PC"))
    );
    let (status, _) = send(
        &fixture.state,
        tailnet_request("POST", "/api/library", Some(json!({ "path": path })))?,
    )
    .await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, state) = send(&fixture.state, tailnet_request("GET", "/api/state", None)?).await?;
    assert_eq!(state["can_open_folders"], json!(false));
    assert_eq!(state["root"], Value::Null);
    let (_, state) = send(&fixture.state, request("GET", "/api/state", None)?).await?;
    assert_eq!(state["can_open_folders"], json!(true));
    let (status, _) = send(&fixture.state, request("GET", "/api/folders", None)?).await?;
    assert_eq!(status, StatusCode::OK);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn update_checks_run_only_from_the_pc_and_only_in_the_desktop_app() -> TestResult {
    let fixture = fixture()?;
    let (_, state) = send(&fixture.state, request("GET", "/api/state", None)?).await?;
    assert_eq!(state["can_check_updates"], json!(false));
    assert_eq!(state["app_version"], json!(env!("CARGO_PKG_VERSION")));
    let (status, _) = send(&fixture.state, request("POST", "/api/update", None)?).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let checks = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = Arc::clone(&checks);
    let installed = fixture.state.update_check.set(Box::new(move || {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0
    }));
    assert!(installed.is_ok());
    let (_, state) = send(&fixture.state, request("GET", "/api/state", None)?).await?;
    assert_eq!(state["can_check_updates"], json!(true));
    let (status, _) = send(
        &fixture.state,
        tailnet_request("POST", "/api/update", None)?,
    )
    .await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, state) = send(&fixture.state, tailnet_request("GET", "/api/state", None)?).await?;
    assert_eq!(state["can_check_updates"], json!(false));
    let (status, body) = send(&fixture.state, request("POST", "/api/update", None)?).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["started"], json!(true));
    let (_, body) = send(&fixture.state, request("POST", "/api/update", None)?).await?;
    assert_eq!(body["started"], json!(false));
    assert_eq!(checks.load(std::sync::atomic::Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn opening_a_missing_folder_is_a_client_error() -> TestResult {
    let fixture = fixture()?;
    let (status, body) = send(
        &fixture.state,
        request(
            "POST",
            "/api/library",
            Some(json!({ "path": "/definitely/not/here" })),
        )?,
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].is_string());
    let (status, _) = send(&fixture.state, request("GET", "/api/photos", None)?).await?;
    assert_eq!(status, StatusCode::CONFLICT);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_moves_deleted_photos_and_writes_sidecars() -> TestResult {
    let fixture = fixture()?;
    let photos = open_and_wait(&fixture).await?;
    let first = photos[0]["id"].as_str().unwrap_or_default().to_owned();
    let second = photos[1]["id"].as_str().unwrap_or_default().to_owned();
    let body = json!({ "changes": [
        { "id": first, "decision": { "mark": "delete", "favorite": false, "rating": 0 } },
        { "id": second, "decision": { "mark": "approved", "favorite": false, "rating": 2 } }
    ]});
    send(
        &fixture.state,
        request("POST", "/api/decisions", Some(body))?,
    )
    .await?;
    let (_, plan) = send(&fixture.state, request("GET", "/api/apply", None)?).await?;
    assert_eq!(
        plan,
        json!({ "sidecars": 2, "deletes": 1, "files_to_move": 2 })
    );
    let (status, report) = send(&fixture.state, request("POST", "/api/apply", None)?).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(report["photos_moved"], json!(1));
    assert!(
        fixture
            .photos
            .join("_to_delete")
            .join("DSCF0001.JPG")
            .is_file()
    );
    assert!(
        fixture
            .photos
            .join("_to_delete")
            .join("DSCF0001.xmp")
            .is_file()
    );
    assert!(fixture.photos.join("DSCF0002.xmp").is_file());
    let (_, photos) = send(&fixture.state, request("GET", "/api/photos", None)?).await?;
    assert_eq!(photos.as_array().map(Vec::len), Some(1));
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn reports_exact_duplicates() -> TestResult {
    let fixture = fixture()?;
    open_and_wait(&fixture).await?;
    let (status, groups) = send(
        &fixture.state,
        request("GET", "/api/duplicates?tolerance=99", None)?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(groups[0]["kind"], json!("exact"));
    assert_eq!(groups[0]["ids"].as_array().map(Vec::len), Some(2));
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn plans_emptying_the_trash_folder() -> TestResult {
    let fixture = fixture()?;
    open_and_wait(&fixture).await?;
    std::fs::create_dir_all(fixture.photos.join("_to_delete"))?;
    std::fs::write(fixture.photos.join("_to_delete").join("OLD.JPG"), b"1234")?;
    let (status, plan) = send(&fixture.state, request("GET", "/api/recycle", None)?).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(plan, json!({ "files": 1, "bytes": 4 }));
    let stale = json!({ "files": 2, "bytes": 9 });
    let (status, _) = send(
        &fixture.state,
        request("POST", "/api/recycle", Some(stale))?,
    )
    .await?;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(fixture.photos.join("_to_delete").join("OLD.JPG").is_file());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn recycling_needs_an_open_folder() -> TestResult {
    let fixture = fixture()?;
    let body = json!({ "files": 0, "bytes": 0 });
    let (status, _) = send(&fixture.state, request("POST", "/api/recycle", Some(body))?).await?;
    assert_eq!(status, StatusCode::CONFLICT);
    Ok(())
}
