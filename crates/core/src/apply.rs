use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::library::{Library, LibraryError};
use crate::model::{Mark, Photo};
use crate::scan::TRASH_DIR;
use crate::sidecar;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Plan {
    pub sidecars: usize,
    pub deletes: usize,
    pub files_to_move: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Failure {
    pub name: String,
    pub error: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Report {
    pub sidecars_written: usize,
    pub photos_moved: usize,
    pub files_moved: usize,
    pub failures: Vec<Failure>,
}

fn needs_sidecar(photo: &Photo) -> bool {
    if !photo.decision.is_empty() {
        return true;
    }
    photo.has_decision && sidecar::sidecar_path(photo).is_some_and(|path| sidecar::exists(&path))
}

fn files_of(photo: &Photo) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = photo.raf.iter().chain(photo.jpg.iter()).cloned().collect();
    if let Some(path) = sidecar::sidecar_path(photo).filter(|path| sidecar::exists(path)) {
        files.push(path);
    }
    files
}

pub fn plan(library: &Library) -> Result<Plan, LibraryError> {
    let photos = library.photos()?;
    let deletes: Vec<&Photo> = photos
        .iter()
        .filter(|photo| photo.decision.mark == Some(Mark::Delete))
        .collect();
    Ok(Plan {
        sidecars: photos.iter().filter(|photo| needs_sidecar(photo)).count(),
        deletes: deletes.len(),
        files_to_move: deletes.iter().map(|photo| files_of(photo).len()).sum(),
    })
}

fn unique_destination(dir: &Path, file_name: &std::ffi::OsStr) -> PathBuf {
    let candidate = dir.join(file_name);
    if std::fs::symlink_metadata(&candidate).is_err() {
        return candidate;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){extension}")))
        .find(|candidate| std::fs::symlink_metadata(candidate).is_err())
        .unwrap_or(candidate)
}

fn trash_dir(photo_dir: &Path) -> Result<PathBuf, String> {
    let trash = photo_dir.join(TRASH_DIR);
    match std::fs::symlink_metadata(&trash) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(trash),
        Ok(_) => Err(format!("{TRASH_DIR} exists but is not a real folder")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => std::fs::create_dir(&trash)
            .map(|()| trash)
            .map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

fn check_inside(root: &Path, file: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(file).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() {
        return Err(format!("{} is not a regular file", file.display()));
    }
    let parent = file
        .parent()
        .and_then(|parent| dunce::canonicalize(parent).ok())
        .ok_or_else(|| format!("cannot resolve the folder of {}", file.display()))?;
    if !parent.starts_with(root) {
        return Err(format!("{} is outside the open folder", file.display()));
    }
    Ok(())
}

fn move_photo(root: &Path, photo: &Photo) -> Result<usize, String> {
    let files = files_of(photo);
    for file in &files {
        check_inside(root, file)?;
    }
    let trash = trash_dir(&photo.dir)?;
    let mut moved = 0;
    for file in &files {
        let Some(name) = file.file_name() else {
            continue;
        };
        let destination = unique_destination(&trash, name);
        std::fs::rename(file, &destination)
            .map_err(|error| format!("cannot move {}: {error}", file.display()))?;
        moved += 1;
    }
    Ok(moved)
}

pub fn apply(library: &Library) -> Result<Report, LibraryError> {
    let photos = library.photos()?;
    let mut report = Report::default();
    for photo in photos.iter().filter(|photo| needs_sidecar(photo)) {
        let Some(path) = sidecar::sidecar_path(photo) else {
            continue;
        };
        match sidecar::write(&path, &photo.decision) {
            Ok(()) => report.sidecars_written += 1,
            Err(error) => report.failures.push(Failure {
                name: photo.name(),
                error: error.to_string(),
            }),
        }
    }
    for photo in photos
        .iter()
        .filter(|photo| photo.decision.mark == Some(Mark::Delete))
    {
        match move_photo(library.root(), photo) {
            Ok(files) => {
                report.photos_moved += 1;
                report.files_moved += files;
            }
            Err(error) => report.failures.push(Failure {
                name: photo.name(),
                error,
            }),
        }
    }
    library.rescan()?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::imaging::fixtures::gradient_jpeg;
    use crate::library::Cache;
    use crate::model::Decision;
    use crate::store::Store;

    fn setup(files: &[&str]) -> Result<(tempfile::TempDir, Library), LibraryError> {
        let dir = tempfile::tempdir()?;
        let photos = dir.path().join("photos");
        std::fs::create_dir_all(&photos)?;
        for name in files {
            std::fs::write(photos.join(name), gradient_jpeg(40, 30, 1))?;
        }
        let store = Arc::new(Store::in_memory()?);
        let cache = Arc::new(Cache::new(dir.path().join("cache"))?);
        let library = Library::open(&photos, store, cache)?;
        Ok((dir, library))
    }

    fn decide(library: &Library, stem: &str, decision: Decision) -> Result<(), LibraryError> {
        let id = library
            .photos()?
            .into_iter()
            .find(|photo| photo.stem == stem)
            .map(|photo| photo.id)
            .unwrap_or_default();
        library.store().set_decision(&id, decision, 1)?;
        Ok(())
    }

    #[test]
    fn moves_delete_pairs_and_writes_sidecars() -> Result<(), LibraryError> {
        let (_dir, library) = setup(&["A.RAF", "A.JPG", "B.JPG", "C.JPG"])?;
        let root = library.root().to_path_buf();
        decide(
            &library,
            "A",
            Decision {
                mark: Some(Mark::Delete),
                favorite: false,
                rating: 0,
            },
        )?;
        decide(
            &library,
            "B",
            Decision {
                mark: Some(Mark::Approved),
                favorite: true,
                rating: 5,
            },
        )?;
        assert_eq!(
            plan(&library)?,
            Plan {
                sidecars: 2,
                deletes: 1,
                files_to_move: 2
            }
        );
        let report = apply(&library)?;
        assert_eq!(report.failures, vec![]);
        assert_eq!(
            (
                report.sidecars_written,
                report.photos_moved,
                report.files_moved
            ),
            (2, 1, 3)
        );
        for name in ["A.RAF", "A.JPG", "A.xmp"] {
            assert!(root.join("_to_delete").join(name).is_file(), "{name} moved");
            assert!(!root.join(name).exists());
        }
        let b = std::fs::read_to_string(root.join("B.xmp"))?;
        assert!(b.contains(r#"xmpDM:pick="1""#) && b.contains(r#"xmp:Label="Purple""#));
        assert!(!root.join("C.xmp").exists());
        assert_eq!(library.photos()?.len(), 2);
        Ok(())
    }

    #[test]
    fn never_overwrites_files_already_in_trash() -> Result<(), LibraryError> {
        let (_dir, library) = setup(&["A.JPG"])?;
        let root = library.root().to_path_buf();
        std::fs::create_dir(root.join("_to_delete"))?;
        std::fs::write(root.join("_to_delete").join("A.JPG"), b"older")?;
        decide(
            &library,
            "A",
            Decision {
                mark: Some(Mark::Delete),
                favorite: false,
                rating: 0,
            },
        )?;
        apply(&library)?;
        assert_eq!(
            std::fs::read(root.join("_to_delete").join("A.JPG"))?,
            b"older"
        );
        assert!(root.join("_to_delete").join("A (1).JPG").is_file());
        Ok(())
    }

    #[test]
    fn refuses_a_trash_path_that_is_a_file() -> Result<(), LibraryError> {
        let (_dir, library) = setup(&["A.JPG"])?;
        let root = library.root().to_path_buf();
        std::fs::write(root.join("_to_delete"), b"not a folder")?;
        decide(
            &library,
            "A",
            Decision {
                mark: Some(Mark::Delete),
                favorite: false,
                rating: 0,
            },
        )?;
        let report = apply(&library)?;
        assert_eq!(report.photos_moved, 0);
        assert_eq!(report.failures.len(), 1);
        assert!(root.join("A.JPG").is_file());
        Ok(())
    }

    #[test]
    fn cleared_decision_updates_an_existing_sidecar() -> Result<(), LibraryError> {
        let (_dir, library) = setup(&["A.JPG"])?;
        let approved = Decision {
            mark: Some(Mark::Approved),
            favorite: false,
            rating: 0,
        };
        decide(&library, "A", approved)?;
        apply(&library)?;
        decide(&library, "A", Decision::default())?;
        assert_eq!(plan(&library)?.sidecars, 1);
        apply(&library)?;
        let content = std::fs::read_to_string(library.root().join("A.xmp"))?;
        assert!(!content.contains("xmpDM:pick"));
        Ok(())
    }
}
