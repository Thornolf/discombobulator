use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use discombobulator_core::library::{Cache, Library};
use discombobulator_core::store::Store;

pub fn run(folder: &Path, fresh: bool) -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let data_dir = if fresh {
        temporary.path().to_path_buf()
    } else {
        discombobulator_core::data_dir().ok_or("no local data folder for this user")?
    };
    discombobulator_core::fsx::create_private_dir(&data_dir)?;
    let store = Arc::new(Store::open(&data_dir.join("library.sqlite3"))?);
    let cache = Arc::new(Cache::new(data_dir.join("cache"))?);

    let started = Instant::now();
    let library = Library::open(folder, store, cache)?;
    let photos = library.photos()?;
    let scanned = started.elapsed();
    let raf = photos.iter().filter(|p| p.raf.is_some()).count();
    let jpg = photos.iter().filter(|p| p.jpg.is_some()).count();
    println!(
        "scan      {} photos ({raf} RAF, {jpg} JPG) in {:.2} s",
        photos.len(),
        scanned.as_secs_f64()
    );

    let processing = Instant::now();
    let report = library.process_all()?;
    let processed = processing.elapsed().as_secs_f64();
    let rate = if processed > 0.0 {
        report.done as f64 / processed
    } else {
        0.0
    };
    println!(
        "process   {} photos in {processed:.2} s ({rate:.1} photos/s, {} failed, {} threads)",
        report.done,
        report.failed,
        std::thread::available_parallelism().map_or(1, usize::from)
    );

    let reading = Instant::now();
    let mut bytes = 0usize;
    for photo in library.photos()? {
        if let Ok(preview) = library
            .cache()
            .read(&photo.id, discombobulator_core::library::Variant::Preview)
        {
            bytes += preview.len();
        }
    }
    println!(
        "previews  {:.1} MB read back in {:.2} s",
        bytes as f64 / 1_048_576.0,
        reading.elapsed().as_secs_f64()
    );
    for photo in library
        .photos()?
        .iter()
        .filter(|p| p.error.is_some())
        .take(10)
    {
        println!(
            "failed    {}: {}",
            photo.name(),
            photo.error.as_deref().unwrap_or_default()
        );
    }
    Ok(())
}
