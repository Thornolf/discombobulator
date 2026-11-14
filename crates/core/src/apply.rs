use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::fsx;
use crate::library::{Library, LibraryError};
use crate::model::{Mark, Photo};
use crate::scan::TRASH_DIR;
use crate::sidecar;

const MAX_SUFFIX: usize = 10_000;

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

#[derive(Debug, PartialEq, Eq)]
enum MoveError {
    Clash,
    Failed(String),
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

fn planned_moves(photo: &Photo) -> usize {
    let images = photo.raf.iter().chain(photo.jpg.iter()).count();
    let sidecar = sidecar::sidecar_path(photo)
        .is_some_and(|path| needs_sidecar(photo) || sidecar::exists(&path));
    images + usize::from(sidecar)
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
        files_to_move: deletes.iter().map(|photo| planned_moves(photo)).sum(),
    })
}

fn photo_folder(root: &Path, photo: &Photo) -> Result<PathBuf, String> {
    let folder = dunce::canonicalize(&photo.dir)
        .map_err(|error| format!("cannot resolve {}: {error}", photo.dir.display()))?;
    if !folder.starts_with(root) {
        return Err(format!(
            "{} is outside the open folder",
            photo.dir.display()
        ));
    }
    Ok(folder)
}

fn write_sidecar(library: &Library, photo: &Photo) -> Result<bool, String> {
    let Some(name) =
        sidecar::sidecar_path(photo).and_then(|path| path.file_name().map(OsStr::to_owned))
    else {
        return Ok(false);
    };
    let path = photo_folder(library.root(), photo)?.join(name);
    let store = library.store();
    let previous = store
        .sidecar_fields(&photo.id)
        .map_err(|error| error.to_string())?;
    let written =
        sidecar::write(&path, &photo.decision, &previous).map_err(|error| error.to_string())?;
    store
        .set_sidecar_fields(&photo.id, &written)
        .map_err(|error| format!("sidecar written but not recorded: {error}"))?;
    Ok(true)
}

fn suffixed(name: &OsStr, n: usize) -> OsString {
    if n == 0 {
        return name.to_owned();
    }
    let path = Path::new(name);
    let mut suffixed = path.file_stem().unwrap_or(name).to_owned();
    suffixed.push(format!(" ({n})"));
    if let Some(extension) = path.extension() {
        suffixed.push(".");
        suffixed.push(extension);
    }
    suffixed
}

fn is_free(path: &Path) -> bool {
    matches!(std::fs::symlink_metadata(path), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
}

fn free_suffix(trash: &Path, names: &[&OsStr], from: usize) -> Result<usize, String> {
    (from..=MAX_SUFFIX)
        .find(|&n| {
            names
                .iter()
                .all(|name| is_free(&trash.join(suffixed(name, n))))
        })
        .ok_or_else(|| format!("no free name left in {}", trash.display()))
}

fn roll_back(done: &[(PathBuf, PathBuf)]) -> Result<(), String> {
    let stuck: Vec<String> = done
        .iter()
        .rev()
        .filter_map(|(from, to)| {
            fsx::rename_noreplace(to, from)
                .err()
                .map(|error| format!("{} is still at {} ({error})", from.display(), to.display()))
        })
        .collect();
    if stuck.is_empty() {
        Ok(())
    } else {
        Err(stuck.join("; "))
    }
}

fn move_files(moves: &[(PathBuf, PathBuf)]) -> Result<(), MoveError> {
    for (index, (from, to)) in moves.iter().enumerate() {
        let Err(error) = fsx::rename_noreplace(from, to) else {
            continue;
        };
        let done = &moves[..index];
        return Err(match roll_back(done) {
            Ok(()) if error.kind() == std::io::ErrorKind::AlreadyExists => MoveError::Clash,
            Ok(()) if done.is_empty() => {
                MoveError::Failed(format!("cannot move {}: {error}", from.display()))
            }
            Ok(()) => MoveError::Failed(format!(
                "cannot move {}: {error}; moved back {}",
                from.display(),
                done.iter()
                    .map(|(from, _)| from.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Err(stuck) => MoveError::Failed(format!(
                "cannot move {}: {error}; could not move back: {stuck}",
                from.display()
            )),
        });
    }
    Ok(())
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
    let folder = photo_folder(root, photo)?;
    let files = files_of(photo);
    for file in &files {
        check_inside(root, file)?;
    }
    let names: Vec<&OsStr> = files
        .iter()
        .map(|file| {
            file.file_name()
                .ok_or_else(|| format!("{} has no file name", file.display()))
        })
        .collect::<Result<_, _>>()?;
    let trash = trash_dir(&folder)?;
    let mut n = 0;
    loop {
        n = free_suffix(&trash, &names, n)?;
        let moves: Vec<(PathBuf, PathBuf)> = files
            .iter()
            .zip(&names)
            .map(|(file, name)| (file.clone(), trash.join(suffixed(name, n))))
            .collect();
        match move_files(&moves) {
            Ok(()) => return Ok(files.len()),
            Err(MoveError::Clash) => n += 1,
            Err(MoveError::Failed(error)) => return Err(error),
        }
    }
}

pub fn apply(library: &Library) -> Result<Report, LibraryError> {
    let photos = library.photos()?;
    let mut report = Report::default();
    for photo in photos.iter().filter(|photo| needs_sidecar(photo)) {
        match write_sidecar(library, photo) {
            Ok(written) => report.sidecars_written += usize::from(written),
            Err(error) => report.failures.push(Failure {
                name: photo.name(),
                error,
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
    if let Err(error) = library.rescan() {
        report.failures.push(Failure {
            name: "rescan".to_owned(),
            error: error.to_string(),
        });
    }
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
                files_to_move: 3
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
        assert!(root.join("_to_delete").join("A (1).xmp").is_file());
        Ok(())
    }

    fn delete() -> Decision {
        Decision {
            mark: Some(Mark::Delete),
            favorite: false,
            rating: 0,
        }
    }

    #[test]
    fn uses_one_suffix_for_every_file_of_a_photo() -> Result<(), LibraryError> {
        let (_dir, library) = setup(&["A.RAF", "A.JPG"])?;
        let root = library.root().to_path_buf();
        let trash = root.join("_to_delete");
        std::fs::create_dir(&trash)?;
        std::fs::write(trash.join("A.JPG"), b"older")?;
        std::fs::write(trash.join("A (1).xmp"), b"older")?;
        decide(&library, "A", delete())?;
        let report = apply(&library)?;
        assert_eq!(report.failures, vec![]);
        for name in ["A (2).RAF", "A (2).JPG", "A (2).xmp"] {
            assert!(trash.join(name).is_file(), "{name} moved");
        }
        assert!(!trash.join("A.RAF").exists());
        assert_eq!(std::fs::read(trash.join("A (1).xmp"))?, b"older");
        Ok(())
    }

    #[test]
    fn moves_files_back_when_a_later_file_fails() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let first = dir.path().join("A.RAF");
        let missing = dir.path().join("A.JPG");
        let trash = dir.path().join("_to_delete");
        std::fs::create_dir(&trash)?;
        std::fs::write(&first, b"raw")?;
        let moves = vec![
            (first.clone(), trash.join("A.RAF")),
            (missing.clone(), trash.join("A.JPG")),
        ];
        let Err(MoveError::Failed(error)) = move_files(&moves) else {
            panic!("the second move must fail");
        };
        assert!(error.contains("moved back"), "{error}");
        assert_eq!(std::fs::read(&first)?, b"raw");
        assert!(!trash.join("A.RAF").exists());
        std::fs::write(&missing, b"jpg")?;
        std::fs::write(trash.join("A.JPG"), b"older")?;
        assert_eq!(move_files(&moves), Err(MoveError::Clash));
        assert!(first.is_file() && missing.is_file());
        assert_eq!(std::fs::read(trash.join("A.JPG"))?, b"older");
        Ok(())
    }

    #[test]
    fn refuses_photos_outside_the_root() -> Result<(), LibraryError> {
        let (dir, library) = setup(&["A.JPG"])?;
        let outside = dir.path().join("outside");
        std::fs::create_dir(&outside)?;
        let photo = library
            .photos()?
            .into_iter()
            .next()
            .map(|photo| Photo {
                dir: outside.clone(),
                ..photo
            })
            .ok_or_else(|| std::io::Error::other("no photo"))?;
        assert!(photo_folder(library.root(), &photo).is_err());
        assert!(write_sidecar(&library, &photo).is_err());
        assert!(move_photo(library.root(), &photo).is_err());
        assert!(!outside.join("_to_delete").exists());
        assert!(!outside.join("A.xmp").exists());
        Ok(())
    }

    #[test]
    fn keeps_lightroom_rating_until_the_app_owns_it() -> Result<(), LibraryError> {
        let (_dir, library) = setup(&["A.JPG"])?;
        let sidecar = library.root().join("A.xmp");
        std::fs::write(
            &sidecar,
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="4"/></rdf:RDF></x:xmpmeta>"#,
        )?;
        let approved = Decision {
            mark: Some(Mark::Approved),
            favorite: false,
            rating: 0,
        };
        decide(&library, "A", approved)?;
        apply(&library)?;
        assert!(std::fs::read_to_string(&sidecar)?.contains(r#"xmp:Rating="4""#));
        decide(
            &library,
            "A",
            Decision {
                rating: 2,
                ..approved
            },
        )?;
        apply(&library)?;
        assert!(std::fs::read_to_string(&sidecar)?.contains(r#"xmp:Rating="2""#));
        decide(&library, "A", approved)?;
        apply(&library)?;
        let content = std::fs::read_to_string(&sidecar)?;
        assert!(!content.contains("Rating"));
        assert!(content.contains(r#"xmpDM:pick="1""#));
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
