use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use rayon::prelude::*;
use serde::Serialize;
use thiserror::Error;

use crate::imaging;
use crate::metadata;
use crate::model::{Decision, Photo};
use crate::raf;
use crate::scan::{self, Source};
use crate::store::{Rendering, Store, StoreError};
use crate::xmp;

pub const MAX_JPEG_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("{0} is not a folder")]
    NotAFolder(PathBuf),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
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

pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub fn new(dir: PathBuf) -> std::io::Result<Cache> {
        std::fs::create_dir_all(&dir)?;
        Ok(Cache { dir })
    }

    pub fn path(&self, id: &str, variant: Variant) -> PathBuf {
        let shard = id.get(..2).unwrap_or("00");
        self.dir
            .join(shard)
            .join(format!("{id}.{}", variant.suffix()))
    }

    pub fn write(&self, id: &str, variant: Variant, bytes: &[u8]) -> std::io::Result<()> {
        let path = self.path(id, variant);
        let Some(parent) = path.parent() else {
            return Err(std::io::Error::other("cache path has no parent"));
        };
        std::fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(bytes)?;
        temporary.persist(&path).map_err(|error| error.error)?;
        Ok(())
    }

    pub fn read(&self, id: &str, variant: Variant) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.path(id, variant))
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

pub fn read_display_jpeg(photo: &Photo) -> Result<Vec<u8>, String> {
    if let Some(jpg) = &photo.jpg {
        let mut bytes = Vec::new();
        File::open(jpg)
            .and_then(|file| file.take(MAX_JPEG_BYTES).read_to_end(&mut bytes))
            .map_err(|error| error.to_string())?;
        return Ok(bytes);
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
        quick_hash: None,
        full_hash: None,
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

pub struct Library {
    root: PathBuf,
    store: Arc<Store>,
    cache: Arc<Cache>,
    progress: Arc<Progress>,
}

impl Library {
    pub fn open(
        root: &Path,
        store: Arc<Store>,
        cache: Arc<Cache>,
    ) -> Result<Library, LibraryError> {
        let root = canonical_folder(root)?;
        let library = Library {
            root,
            store,
            cache,
            progress: Arc::new(Progress::default()),
        };
        library.rescan()?;
        Ok(library)
    }

    pub fn rescan(&self) -> Result<usize, LibraryError> {
        let photos: Vec<Photo> = scan::scan(&self.root)
            .iter()
            .map(photo_from_source)
            .collect();
        self.store.replace_photos(&self.root, &photos)?;
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

    pub fn process_all(&self) -> Result<ProgressSnapshot, LibraryError> {
        let pending: Vec<Photo> = self
            .photos()?
            .into_iter()
            .filter(|photo| !photo.is_processed() || !self.has_cached(photo))
            .collect();
        self.progress.total.store(pending.len(), Ordering::Relaxed);
        self.progress.done.store(0, Ordering::Relaxed);
        self.progress.failed.store(0, Ordering::Relaxed);
        self.progress.running.store(true, Ordering::Relaxed);
        pending.par_iter().for_each(|photo| {
            let rendering = self.render(photo);
            if rendering.error.is_some() {
                self.progress.failed.fetch_add(1, Ordering::Relaxed);
            }
            if self.store.save_rendering(&photo.id, &rendering).is_err() {
                self.progress.failed.fetch_add(1, Ordering::Relaxed);
            }
            self.progress.done.fetch_add(1, Ordering::Relaxed);
        });
        self.progress.running.store(false, Ordering::Relaxed);
        Ok(self.progress.snapshot())
    }

    fn has_cached(&self, photo: &Photo) -> bool {
        photo.error.is_some()
            || (self.cache.path(&photo.id, Variant::Thumb).is_file()
                && self.cache.path(&photo.id, Variant::Preview).is_file())
    }

    pub fn render(&self, photo: &Photo) -> Rendering {
        let failed = |error: String| Rendering {
            fingerprint: photo.fingerprint.clone(),
            capture_ms: None,
            orientation: 1,
            dhash: None,
            camera_rating: None,
            width: None,
            height: None,
            error: Some(error),
        };
        let jpeg = match read_display_jpeg(photo) {
            Ok(jpeg) => jpeg,
            Err(error) => return failed(error),
        };
        let info = metadata::capture_info(&jpeg);
        let camera_rating = xmp::rating(&jpeg).or_else(|| {
            photo
                .raf
                .as_ref()
                .and_then(|raf| raf::read_head(raf, xmp::XMP_SCAN_LIMIT).ok())
                .and_then(|head| xmp::rating(&head))
        });
        let rendered = match imaging::render(&jpeg, info.orientation) {
            Ok(rendered) => rendered,
            Err(error) => return failed(error.to_string()),
        };
        let written = self
            .cache
            .write(&photo.id, Variant::Thumb, &rendered.thumb)
            .and_then(|()| {
                self.cache
                    .write(&photo.id, Variant::Preview, &rendered.preview)
            });
        if let Err(error) = written {
            return failed(error.to_string());
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
        let Some(photo) = self.photo(id)? else {
            return Ok(None);
        };
        if let Ok(bytes) = self.cache.read(id, variant) {
            return Ok(Some(bytes));
        }
        let rendering = self.render(&photo);
        self.store.save_rendering(id, &rendering)?;
        Ok(self.cache.read(id, variant).ok())
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
    fn rejects_missing_folder() {
        let store = Store::in_memory().map(Arc::new);
        let cache = tempfile::tempdir()
            .and_then(|dir| Cache::new(dir.path().to_path_buf()))
            .map(Arc::new);
        if let (Ok(store), Ok(cache)) = (store, cache) {
            assert!(matches!(
                Library::open(Path::new("/definitely/not/here"), store, cache),
                Err(LibraryError::NotAFolder(_))
            ));
        }
    }
}
