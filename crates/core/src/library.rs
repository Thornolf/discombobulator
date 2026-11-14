use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use rayon::prelude::*;
use serde::Serialize;
use thiserror::Error;

use crate::fsx;
use crate::imaging;
use crate::metadata::{self, CaptureInfo};
use crate::model::{Decision, Photo};
use crate::raf;
use crate::scan::{self, Source};
use crate::store::{Rendering, Store, StoreError};
use crate::xmp;

pub const MAX_JPEG_BYTES: u64 = 64 * 1024 * 1024;
const PAIRED_RAF_XMP_SCAN: u64 = 512 * 1024;
const MAX_WORKERS: usize = 8;

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("{0} is not a folder")]
    NotAFolder(PathBuf),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("cannot start workers: {0}")]
    Workers(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Thumb,
    Preview,
}

impl Variant {
    fn suffix(self) -> &'static str {
        match self {
            Variant::Thumb => "thumb.jpg",
            Variant::Preview => "preview.jpg",
        }
    }
}

pub fn is_photo_id(id: &str) -> bool {
    id.len() == 20
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub fn new(dir: PathBuf) -> std::io::Result<Cache> {
        fsx::create_private_dir(&dir)?;
        Ok(Cache { dir })
    }

    fn checked_path(&self, id: &str, variant: Variant) -> std::io::Result<PathBuf> {
        if !is_photo_id(id) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid photo id",
            ));
        }
        Ok(self
            .dir
            .join(&id[..2])
            .join(format!("{id}.{}", variant.suffix())))
    }

    pub fn has(&self, id: &str, variant: Variant) -> bool {
        self.checked_path(id, variant)
            .is_ok_and(|path| path.is_file())
    }

    pub fn write(&self, id: &str, variant: Variant, bytes: &[u8]) -> std::io::Result<()> {
        let path = self.checked_path(id, variant)?;
        let Some(parent) = path.parent() else {
            return Err(std::io::Error::other("cache path has no parent"));
        };
        fsx::create_private_dir(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(bytes)?;
        temporary.persist(&path).map_err(|error| error.error)?;
        Ok(())
    }

    pub fn read(&self, id: &str, variant: Variant) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.checked_path(id, variant)?)
    }
}

#[derive(Debug, Default)]
pub struct Progress {
    total: AtomicUsize,
    done: AtomicUsize,
    failed: AtomicUsize,
    running: AtomicBool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ProgressSnapshot {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub running: bool,
}

impl Progress {
    pub fn snapshot(&self) -> ProgressSnapshot {
        ProgressSnapshot {
            total: self.total.load(Ordering::Relaxed),
            done: self.done.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            running: self.running.load(Ordering::Relaxed),
        }
    }
}

struct RunningGuard<'a>(&'a AtomicBool);

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

pub fn read_display_jpeg(photo: &Photo) -> Result<Vec<u8>, String> {
    if let Some(jpg) = &photo.jpg {
        return fsx::read_limited(jpg, MAX_JPEG_BYTES).map_err(|error| error.to_string());
    }
    let raf = photo.raf.as_ref().ok_or("photo has no file")?;
    raf::read_embedded_jpeg(raf).map_err(|error| error.to_string())
}

fn photo_from_source(source: &Source) -> Photo {
    Photo {
        id: source.id(),
        dir: source.dir.clone(),
        stem: source.stem.clone(),
        raf: source.raf.as_ref().map(|file| file.path.clone()),
        jpg: source.jpg.as_ref().map(|file| file.path.clone()),
        size: source.size(),
        modified_ms: source.modified_ms(),
        fingerprint: source.fingerprint(),
        processed_fingerprint: None,
        capture_ms: None,
        orientation: 1,
        dhash: None,
        camera_rating: None,
        width: None,
        height: None,
        error: None,
        decision: Decision::default(),
    }
}

pub fn canonical_folder(path: &Path) -> Result<PathBuf, LibraryError> {
    let canonical =
        dunce::canonicalize(path).map_err(|_| LibraryError::NotAFolder(path.to_path_buf()))?;
    if !canonical.is_dir() {
        return Err(LibraryError::NotAFolder(path.to_path_buf()));
    }
    Ok(canonical)
}

fn workers() -> usize {
    std::thread::available_parallelism()
        .map_or(1, usize::from)
        .clamp(1, MAX_WORKERS)
}

pub struct Library {
    root: PathBuf,
    store: Arc<Store>,
    cache: Arc<Cache>,
    progress: Arc<Progress>,
    pool: rayon::ThreadPool,
}

impl Library {
    pub fn open(
        root: &Path,
        store: Arc<Store>,
        cache: Arc<Cache>,
    ) -> Result<Library, LibraryError> {
        let root = canonical_folder(root)?;
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers())
            .build()
            .map_err(|error| LibraryError::Workers(error.to_string()))?;
        let library = Library {
            root,
            store,
            cache,
            progress: Arc::new(Progress::default()),
            pool,
        };
        library.rescan()?;
        Ok(library)
    }

    pub fn rescan(&self) -> Result<usize, LibraryError> {
        let scan = scan::scan(&self.root)?;
        let photos: Vec<Photo> = scan.sources.iter().map(photo_from_source).collect();
        self.store
            .replace_photos(&self.root, &photos, scan.errors == 0)?;
        Ok(photos.len())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub fn cache(&self) -> &Arc<Cache> {
        &self.cache
    }

    pub fn progress(&self) -> ProgressSnapshot {
        self.progress.snapshot()
    }

    pub fn photos(&self) -> Result<Vec<Photo>, LibraryError> {
        let mut photos = self.store.photos_under(&self.root)?;
        photos.sort_by(|a, b| {
            a.time_ms()
                .cmp(&b.time_ms())
                .then_with(|| a.dir.cmp(&b.dir))
                .then_with(|| a.stem.cmp(&b.stem))
        });
        Ok(photos)
    }

    pub fn photo(&self, id: &str) -> Result<Option<Photo>, LibraryError> {
        Ok(self
            .store
            .photo(id)?
            .filter(|photo| photo.dir.starts_with(&self.root)))
    }

    fn needs_rendering(&self, photo: &Photo) -> bool {
        !photo.is_processed()
            || photo.error.is_some()
            || !self.cache.has(&photo.id, Variant::Thumb)
            || !self.cache.has(&photo.id, Variant::Preview)
    }

    pub fn process_all(&self) -> Result<ProgressSnapshot, LibraryError> {
        let pending: Vec<Photo> = self
            .photos()?
            .into_iter()
            .filter(|photo| self.needs_rendering(photo))
            .collect();
        if self
            .progress
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(self.progress.snapshot());
        }
        self.progress.total.store(pending.len(), Ordering::Relaxed);
        self.progress.done.store(0, Ordering::Relaxed);
        self.progress.failed.store(0, Ordering::Relaxed);
        let _running = RunningGuard(&self.progress.running);
        self.pool.install(|| {
            pending.par_iter().for_each(|photo| {
                let rendering = self.render_safely(photo);
                let saved = self.store.save_rendering(&photo.id, &rendering).is_ok();
                if rendering.error.is_some() || !saved {
                    self.progress.failed.fetch_add(1, Ordering::Relaxed);
                }
                self.progress.done.fetch_add(1, Ordering::Relaxed);
            });
        });
        Ok(self.progress.snapshot())
    }

    fn render_safely(&self, photo: &Photo) -> Rendering {
        catch_unwind(AssertUnwindSafe(|| self.render(photo))).unwrap_or_else(|_| {
            failed_rendering(
                photo,
                CaptureInfo::default(),
                "decoder crashed on this file",
            )
        })
    }

    fn camera_rating(photo: &Photo, jpeg: &[u8]) -> Option<u8> {
        xmp::rating(jpeg).or_else(|| {
            photo.jpg.as_ref()?;
            let raf = photo.raf.as_ref()?;
            let head = fsx::read_limited(raf, PAIRED_RAF_XMP_SCAN).ok()?;
            xmp::rating(&head)
        })
    }

    pub fn render(&self, photo: &Photo) -> Rendering {
        let jpeg = match read_display_jpeg(photo) {
            Ok(jpeg) => jpeg,
            Err(error) => return failed_rendering(photo, CaptureInfo::default(), &error),
        };
        let info = metadata::capture_info(&jpeg);
        let camera_rating = Library::camera_rating(photo, &jpeg);
        let rendered = match imaging::render(&jpeg, info.orientation) {
            Ok(rendered) => rendered,
            Err(error) => {
                return Rendering {
                    camera_rating,
                    ..failed_rendering(photo, info, &error.to_string())
                };
            }
        };
        let written = self
            .cache
            .write(&photo.id, Variant::Thumb, &rendered.thumb)
            .and_then(|()| {
                self.cache
                    .write(&photo.id, Variant::Preview, &rendered.preview)
            });
        if let Err(error) = written {
            return Rendering {
                camera_rating,
                ..failed_rendering(photo, info, &error.to_string())
            };
        }
        Rendering {
            fingerprint: photo.fingerprint.clone(),
            capture_ms: info.capture_ms,
            orientation: info.orientation,
            dhash: Some(rendered.dhash),
            camera_rating,
            width: Some(rendered.width),
            height: Some(rendered.height),
            error: None,
        }
    }

    pub fn image(&self, id: &str, variant: Variant) -> Result<Option<Vec<u8>>, LibraryError> {
        if !is_photo_id(id) {
            return Ok(None);
        }
        let Some(photo) = self.photo(id)? else {
            return Ok(None);
        };
        if photo.is_processed() {
            if photo.error.is_some() {
                return Ok(None);
            }
            if let Ok(bytes) = self.cache.read(id, variant) {
                return Ok(Some(bytes));
            }
        }
        let rendering = self.render_safely(&photo);
        self.store.save_rendering(id, &rendering)?;
        Ok(self.cache.read(id, variant).ok())
    }
}

fn failed_rendering(photo: &Photo, info: CaptureInfo, error: &str) -> Rendering {
    Rendering {
        fingerprint: photo.fingerprint.clone(),
        capture_ms: info.capture_ms,
        orientation: info.orientation,
        dhash: None,
        camera_rating: None,
        width: None,
        height: None,
        error: Some(error.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imaging::fixtures::gradient_jpeg;
    use crate::raf::fixtures::raf_with_jpeg;

    fn library_with(
        files: &[(&str, Vec<u8>)],
    ) -> Result<(tempfile::TempDir, Library), LibraryError> {
        let dir = tempfile::tempdir()?;
        let photos = dir.path().join("photos");
        for (name, bytes) in files {
            let path = photos.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, bytes)?;
        }
        std::fs::create_dir_all(&photos)?;
        let store = Arc::new(Store::in_memory()?);
        let cache = Arc::new(Cache::new(dir.path().join("cache"))?);
        let library = Library::open(&photos, store, cache)?;
        Ok((dir, library))
    }

    #[test]
    fn processes_raf_only_and_jpg_photos() -> Result<(), LibraryError> {
        let (_dir, library) = library_with(&[
            ("A.RAF", raf_with_jpeg(&gradient_jpeg(400, 300, 1))),
            ("B.JPG", gradient_jpeg(300, 400, 2)),
        ])?;
        let report = library.process_all()?;
        assert_eq!((report.total, report.done, report.failed), (2, 2, 0));
        let photos = library.photos()?;
        assert!(photos.iter().all(Photo::is_processed));
        assert!(photos.iter().all(|p| p.dhash.is_some()));
        for photo in &photos {
            assert!(library.image(&photo.id, Variant::Thumb)?.is_some());
        }
        Ok(())
    }

    #[test]
    fn second_run_skips_processed_photos() -> Result<(), LibraryError> {
        let (_dir, library) = library_with(&[("B.JPG", gradient_jpeg(300, 200, 2))])?;
        library.process_all()?;
        library.rescan()?;
        assert_eq!(library.process_all()?.total, 0);
        Ok(())
    }

    #[test]
    fn broken_file_is_reported_not_fatal() -> Result<(), LibraryError> {
        let (_dir, library) = library_with(&[
            ("bad.RAF", b"FUJIFILMCCD-RAW nope".to_vec()),
            ("good.JPG", gradient_jpeg(100, 100, 3)),
        ])?;
        let report = library.process_all()?;
        assert_eq!((report.done, report.failed), (2, 1));
        let bad = library.photos()?.into_iter().find(|p| p.stem == "bad");
        assert!(bad.and_then(|p| p.error).is_some());
        Ok(())
    }

    #[test]
    fn photo_lookup_is_limited_to_the_open_root() -> Result<(), LibraryError> {
        let (dir, library) = library_with(&[("A.JPG", gradient_jpeg(50, 50, 1))])?;
        let other_root = dir.path().join("other");
        std::fs::create_dir_all(&other_root)?;
        std::fs::write(other_root.join("Z.JPG"), gradient_jpeg(50, 50, 2))?;
        let other = Library::open(
            &other_root,
            Arc::clone(library.store()),
            Arc::clone(library.cache()),
        )?;
        let foreign = other.photos()?.remove(0).id;
        assert!(library.photo(&foreign)?.is_none());
        Ok(())
    }

    #[test]
    fn rejects_missing_folder() -> Result<(), LibraryError> {
        let dir = tempfile::tempdir()?;
        let store = Arc::new(Store::in_memory()?);
        let cache = Arc::new(Cache::new(dir.path().join("cache"))?);
        assert!(matches!(
            Library::open(&dir.path().join("missing"), store, cache),
            Err(LibraryError::NotAFolder(_))
        ));
        Ok(())
    }

    #[test]
    fn missing_root_keeps_decisions() -> Result<(), LibraryError> {
        let (dir, library) = library_with(&[("A.JPG", gradient_jpeg(40, 30, 1))])?;
        let id = library.photos()?.remove(0).id;
        let delete = Decision {
            mark: Some(crate::model::Mark::Delete),
            favorite: false,
            rating: 0,
        };
        library.store().set_decision(&id, delete, 1)?;
        let moved = dir.path().join("away");
        std::fs::rename(library.root(), &moved)?;
        assert!(library.rescan().is_err());
        std::fs::rename(&moved, library.root())?;
        library.rescan()?;
        assert_eq!(library.photo(&id)?.map(|p| p.decision), Some(delete));
        Ok(())
    }

    #[test]
    fn pair_is_displayed_from_its_jpg() -> Result<(), LibraryError> {
        let (_dir, library) = library_with(&[
            ("A.RAF", raf_with_jpeg(&gradient_jpeg(400, 300, 1))),
            ("A.JPG", gradient_jpeg(200, 100, 2)),
        ])?;
        library.process_all()?;
        let photo = library.photos()?.remove(0);
        assert_eq!((photo.width, photo.height), (Some(200), Some(100)));
        Ok(())
    }

    #[test]
    fn changed_file_is_processed_again() -> Result<(), LibraryError> {
        let (_dir, library) = library_with(&[("A.JPG", gradient_jpeg(100, 50, 1))])?;
        library.process_all()?;
        std::fs::write(library.root().join("A.JPG"), gradient_jpeg(60, 120, 2))?;
        library.rescan()?;
        assert_eq!(library.process_all()?.total, 1);
        let photo = library.photos()?.remove(0);
        assert_eq!((photo.width, photo.height), (Some(60), Some(120)));
        Ok(())
    }

    #[test]
    fn failed_photos_are_retried() -> Result<(), LibraryError> {
        let (_dir, library) = library_with(&[("bad.JPG", b"not a jpeg".to_vec())])?;
        assert_eq!(library.process_all()?.failed, 1);
        assert_eq!(library.process_all()?.total, 1);
        Ok(())
    }

    #[test]
    fn camera_rating_comes_from_embedded_xmp() -> Result<(), LibraryError> {
        let mut jpeg = gradient_jpeg(40, 30, 1);
        jpeg.extend_from_slice(br#"<x:xmpmeta xmp:Rating="4"></x:xmpmeta>"#);
        let (_dir, library) = library_with(&[("A.JPG", jpeg)])?;
        library.process_all()?;
        assert_eq!(library.photos()?.remove(0).camera_rating, Some(4));
        Ok(())
    }

    #[test]
    fn unknown_or_malformed_ids_have_no_image() -> Result<(), LibraryError> {
        let (_dir, library) = library_with(&[("A.JPG", gradient_jpeg(40, 30, 1))])?;
        assert_eq!(library.image("0123456789abcdef0123", Variant::Thumb)?, None);
        assert_eq!(library.image("../../etc/passwd", Variant::Thumb)?, None);
        Ok(())
    }

    #[test]
    fn cache_rejects_ids_that_are_not_photo_ids() {
        let cache = tempfile::tempdir().and_then(|dir| Cache::new(dir.path().to_path_buf()));
        if let Ok(cache) = cache {
            assert!(cache.write("../escape", Variant::Thumb, b"x").is_err());
            assert!(cache.read("/etc/passwd", Variant::Thumb).is_err());
        }
    }
}
