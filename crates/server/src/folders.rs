use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct Listing {
    pub path: Option<String>,
    pub parent: Option<String>,
    pub entries: Vec<Entry>,
}

fn entry(name: String, path: &Path) -> Entry {
    Entry {
        name,
        path: path.to_string_lossy().into_owned(),
    }
}

pub fn roots() -> Vec<Entry> {
    let mut entries = Vec::new();
    if let Some(dirs) = directories::UserDirs::new() {
        if let Some(pictures) = dirs.picture_dir() {
            entries.push(entry("Pictures".into(), pictures));
        }
        entries.push(entry("Home".into(), dirs.home_dir()));
    }
    if cfg!(windows) {
        for letter in 'A'..='Z' {
            let drive = PathBuf::from(format!("{letter}:\\"));
            if drive.is_dir() {
                entries.push(entry(format!("{letter}:"), &drive));
            }
        }
    } else {
        entries.push(entry("/".into(), Path::new("/")));
    }
    entries
}

pub fn list(path: &Path) -> std::io::Result<Listing> {
    let canonical = dunce::canonicalize(path)?;
    let mut entries: Vec<Entry> = std::fs::read_dir(&canonical)?
        .filter_map(Result::ok)
        .filter(|item| item.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|item| {
            let name = item.file_name().to_string_lossy().into_owned();
            (!name.starts_with('.') && !name.starts_with('$')).then(|| entry(name, &item.path()))
        })
        .collect();
    entries.sort_by_key(|item| item.name.to_lowercase());
    Ok(Listing {
        path: Some(canonical.to_string_lossy().into_owned()),
        parent: canonical
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned()),
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_visible_subfolders_sorted_without_files() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        for name in ["b", "A", ".hidden"] {
            std::fs::create_dir(dir.path().join(name))?;
        }
        std::fs::write(dir.path().join("file.JPG"), b"x")?;
        let names: Vec<String> = list(dir.path())?
            .entries
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, vec!["A".to_owned(), "b".to_owned()]);
        Ok(())
    }

    #[test]
    fn missing_folder_is_an_error() {
        assert!(list(Path::new("/definitely/not/here")).is_err());
    }
}
