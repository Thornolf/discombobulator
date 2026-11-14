use std::path::{Path, PathBuf};

use serde::Serialize;
use walkdir::WalkDir;

use crate::apply::Failure;
use crate::recycle_bin;
use crate::scan::TRASH_DIR;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct RecyclePlan {
    pub files: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RecycleReport {
    pub recycled: usize,
    pub bytes: u64,
    pub failures: Vec<Failure>,
}

fn is_trash_dir(entry: &walkdir::DirEntry) -> bool {
    entry.depth() > 0
        && entry.file_type().is_dir()
        && entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(TRASH_DIR)
}

pub fn pending_files(root: &Path) -> Vec<(PathBuf, u64)> {
    let mut files = Vec::new();
    let folders = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry.file_type().is_dir()
                || !entry.file_name().to_string_lossy().starts_with('.')
        })
        .filter_map(Result::ok)
        .filter(is_trash_dir);
    for folder in folders {
        for entry in WalkDir::new(folder.path())
            .follow_links(false)
            .min_depth(1)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            if let Ok(metadata) = entry.metadata() {
                files.push((entry.path().to_path_buf(), metadata.len()));
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

pub fn plan(root: &Path) -> RecyclePlan {
    let files = pending_files(root);
    RecyclePlan {
        files: files.len(),
        bytes: files.iter().map(|(_, size)| size).sum(),
    }
}

fn checked_path(root: &Path, file: &Path) -> Result<PathBuf, String> {
    let name = file.file_name().ok_or("has no name")?;
    let parent = file
        .parent()
        .and_then(|parent| dunce::canonicalize(parent).ok())
        .ok_or("its folder cannot be resolved")?;
    let in_trash = parent
        .strip_prefix(root)
        .map(|relative| {
            relative
                .components()
                .any(|part| part.as_os_str().eq_ignore_ascii_case(TRASH_DIR))
        })
        .unwrap_or(false);
    if !in_trash {
        return Err(format!(
            "not inside a {TRASH_DIR} folder of the open folder"
        ));
    }
    let resolved = parent.join(name);
    let metadata =
        std::fs::symlink_metadata(&resolved).map_err(|error| error.kind().to_string())?;
    if !metadata.file_type().is_file() {
        return Err("not a regular file".into());
    }
    Ok(resolved)
}

pub fn recycle_with<F>(root: &Path, mut send: F) -> RecycleReport
where
    F: FnMut(&Path, u64) -> Result<(), String>,
{
    let mut report = RecycleReport::default();
    for (file, size) in pending_files(root) {
        let name = file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        match checked_path(root, &file).and_then(|resolved| send(&resolved, size)) {
            Ok(()) => {
                report.recycled += 1;
                report.bytes += size;
            }
            Err(error) => report.failures.push(Failure { name, error }),
        }
    }
    report
}

pub fn recycle(root: &Path) -> RecycleReport {
    let limit = recycle_bin::max_file_bytes(root);
    recycle_with(root, |file, size| {
        let limit = limit.clone()?;
        if size > limit {
            return Err("larger than the Recycle Bin allows, so it was kept".into());
        }
        trash::delete(file)
            .map_err(|_| "could not be sent to the Recycle Bin, so it was kept".into())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)
    }

    #[test]
    fn plans_only_files_inside_trash_folders() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dunce::canonicalize(dir.path())?;
        write(&root.join("A.JPG"), b"keep")?;
        write(&root.join("_to_delete/B.JPG"), b"12345")?;
        write(&root.join("day2/_to_delete/C.RAF"), b"123")?;
        write(&root.join(".hidden/_to_delete/D.JPG"), b"1")?;
        assert_eq!(plan(&root), RecyclePlan { files: 2, bytes: 8 });
        Ok(())
    }

    #[test]
    fn recycles_through_the_given_sender_and_reports_failures() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dunce::canonicalize(dir.path())?;
        write(&root.join("_to_delete/B.JPG"), b"12345")?;
        write(&root.join("_to_delete/C.RAF"), b"123")?;
        let mut sent = Vec::new();
        let report = recycle_with(&root, |file, _| {
            if file.ends_with("C.RAF") {
                return Err("locked".into());
            }
            sent.push(file.to_path_buf());
            Ok(())
        });
        assert_eq!((report.recycled, report.bytes), (1, 5));
        assert_eq!(report.failures.len(), 1);
        assert_eq!(sent, vec![root.join("_to_delete").join("B.JPG")]);
        Ok(())
    }

    #[test]
    fn an_open_folder_named_like_the_trash_is_not_the_trash() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dunce::canonicalize(dir.path())?.join("_to_delete");
        write(&root.join("A.JPG"), b"keep")?;
        let root = dunce::canonicalize(&root)?;
        assert_eq!(plan(&root), RecyclePlan::default());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn never_follows_links_out_of_the_trash_folder() -> std::io::Result<()> {
        let outside = tempfile::tempdir()?;
        write(&outside.path().join("precious.JPG"), b"x")?;
        let dir = tempfile::tempdir()?;
        let root = dunce::canonicalize(dir.path())?;
        std::fs::create_dir_all(root.join("_to_delete"))?;
        std::os::unix::fs::symlink(outside.path(), root.join("_to_delete").join("link"))?;
        std::os::unix::fs::symlink(
            outside.path().join("precious.JPG"),
            root.join("_to_delete").join("file.JPG"),
        )?;
        let report = recycle_with(&root, |_, _| Err("should not be called".into()));
        assert_eq!(report.recycled, 0);
        assert!(report.failures.is_empty());
        assert!(outside.path().join("precious.JPG").exists());
        Ok(())
    }
}
