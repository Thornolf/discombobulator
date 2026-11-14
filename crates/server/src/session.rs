use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use discombobulator_core::library::{Cache, Library, LibraryError};
use discombobulator_core::scan::TRASH_DIR;
use discombobulator_core::store::Store;
use notify::{RecursiveMode, Watcher};

const QUIET_PERIOD: Duration = Duration::from_millis(1500);

pub struct Session {
    pub library: Arc<Library>,
    pub version: Arc<AtomicU64>,
    processing: Arc<Processing>,
    dirty: Arc<Mutex<Option<Instant>>>,
    closed: Arc<AtomicBool>,
    _watcher: Option<notify::RecommendedWatcher>,
}

impl Session {
    pub fn open(
        root: &Path,
        store: Arc<Store>,
        cache: Arc<Cache>,
        version: Arc<AtomicU64>,
    ) -> Result<Session, LibraryError> {
        let library = Arc::new(Library::open(root, store, cache)?);
        let dirty: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
        let watcher = start_watcher(library.root(), Arc::clone(&dirty));
        let session = Session {
            library,
            version,
            processing: Arc::new(Processing::default()),
            dirty,
            closed: Arc::new(AtomicBool::new(false)),
            _watcher: watcher,
        };
        session.version.fetch_add(1, Ordering::Relaxed);
        session.spawn_processing();
        session.spawn_rescan_loop();
        Ok(session)
    }

    pub fn spawn_processing(&self) {
        start_processing(&self.library, &self.processing, &self.version);
    }

    fn spawn_rescan_loop(&self) {
        let library = Arc::clone(&self.library);
        let dirty = Arc::clone(&self.dirty);
        let closed = Arc::clone(&self.closed);
        let version = Arc::clone(&self.version);
        let processing = Arc::clone(&self.processing);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(500));
            while !closed.load(Ordering::Relaxed) {
                ticker.tick().await;
                let due = dirty
                    .lock()
                    .map(|mut last| match *last {
                        Some(at) if at.elapsed() >= QUIET_PERIOD => last.take().is_some(),
                        _ => false,
                    })
                    .unwrap_or(false);
                if !due || closed.load(Ordering::Relaxed) {
                    continue;
                }
                let scan_library = Arc::clone(&library);
                if tokio::task::spawn_blocking(move || scan_library.rescan())
                    .await
                    .is_ok()
                {
                    version.fetch_add(1, Ordering::Relaxed);
                }
                start_processing(&library, &processing, &version);
            }
        });
    }

    pub fn is_processing(&self) -> bool {
        self.processing.running.load(Ordering::Relaxed)
    }
}

#[derive(Default)]
struct Processing {
    running: AtomicBool,
    again: AtomicBool,
}

fn start_processing(
    library: &Arc<Library>,
    processing: &Arc<Processing>,
    version: &Arc<AtomicU64>,
) {
    processing.again.store(true, Ordering::Release);
    if processing.running.swap(true, Ordering::AcqRel) {
        return;
    }
    let library = Arc::clone(library);
    let processing = Arc::clone(processing);
    let version = Arc::clone(version);
    tokio::task::spawn_blocking(move || {
        loop {
            while processing.again.swap(false, Ordering::AcqRel) {
                let _ = library.process_all();
                version.fetch_add(1, Ordering::Relaxed);
            }
            processing.running.store(false, Ordering::Release);
            if !processing.again.load(Ordering::Acquire)
                || processing.running.swap(true, Ordering::AcqRel)
            {
                break;
            }
        }
    });
}

impl Drop for Session {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
    }
}

fn is_relevant(path: &Path) -> bool {
    let in_trash = path
        .components()
        .any(|part| part.as_os_str().eq_ignore_ascii_case(TRASH_DIR));
    !in_trash && discombobulator_core::scan::kind_of(path).is_some()
}

fn start_watcher(
    root: &Path,
    dirty: Arc<Mutex<Option<Instant>>>,
) -> Option<notify::RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else { return };
        if event.paths.iter().any(|path| is_relevant(path))
            && let Ok(mut last) = dirty.lock()
        {
            *last = Some(Instant::now());
        }
    })
    .ok()?;
    watcher.watch(root, RecursiveMode::Recursive).ok()?;
    Some(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_photo_files_outside_trash_trigger_rescans() {
        assert!(is_relevant(Path::new("/p/DSCF0001.RAF")));
        assert!(is_relevant(Path::new("/p/sub/DSCF0001.jpg")));
        assert!(!is_relevant(Path::new("/p/DSCF0001.xmp")));
        assert!(!is_relevant(Path::new("/p/_to_delete/DSCF0001.RAF")));
    }
}
