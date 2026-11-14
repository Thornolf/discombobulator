use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use walkdir::WalkDir;

pub const TRASH_DIR: &str = "_to_delete";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Raf,
    Jpg,
}

pub fn kind_of(path: &Path) -> Option<Kind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "raf" => Some(Kind::Raf),
        "jpg" | "jpeg" => Some(Kind::Jpg),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStamp {
    pub path: PathBuf,
    pub size: u64,
    pub modified_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub dir: PathBuf,
    pub stem: String,
    pub raf: Option<FileStamp>,
    pub jpg: Option<FileStamp>,
}

impl Source {
    pub fn files(&self) -> impl Iterator<Item = &FileStamp> {
        self.raf.iter().chain(self.jpg.iter())
    }

    pub fn display_file(&self) -> Option<&FileStamp> {
        self.jpg.as_ref().or(self.raf.as_ref())
    }

    pub fn anchor(&self) -> Option<&FileStamp> {
        self.raf.as_ref().or(self.jpg.as_ref())
    }

    pub fn fingerprint(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        for file in self.files() {
            hasher.update(file.path.to_string_lossy().as_bytes());
            hasher.update(&file.size.to_le_bytes());
            hasher.update(&file.modified_ms.to_le_bytes());
        }
        hasher.finalize().to_hex()[..16].to_owned()
    }

    pub fn id(&self) -> String {
        let name = self
            .anchor()
            .and_then(|file| file.path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.stem.clone());
        photo_id(&self.dir, &name)
    }

    pub fn size(&self) -> u64 {
        self.files().map(|file| file.size).sum()
    }

    pub fn modified_ms(&self) -> i64 {
        self.files().map(|file| file.modified_ms).max().unwrap_or(0)
    }
}

pub fn photo_id(dir: &Path, anchor_name: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(dir.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(anchor_name.as_bytes());
    hasher.finalize().to_hex()[..20].to_owned()
}

fn is_skipped_dir(name: &str) -> bool {
    name.starts_with('.') || name.eq_ignore_ascii_case(TRASH_DIR)
}

#[derive(Debug, Default)]
pub struct Scan {
    pub sources: Vec<Source>,
    pub errors: usize,
}

fn stamp(entry: &walkdir::DirEntry) -> Option<FileStamp> {
    let metadata = entry.metadata().ok()?;
    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(0);
    Some(FileStamp {
        path: entry.path().to_path_buf(),
        size: metadata.len(),
        modified_ms,
    })
}

fn jpg_rank(file: &FileStamp) -> (bool, PathBuf) {
    let is_jpeg = file
        .path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jpeg"));
    (is_jpeg, file.path.clone())
}

fn single(dir: &Path, stem: &str, kind: Kind, file: FileStamp) -> Source {
    let (raf, jpg) = match kind {
        Kind::Raf => (Some(file), None),
        Kind::Jpg => (None, Some(file)),
    };
    Source {
        dir: dir.to_path_buf(),
        stem: stem.to_owned(),
        raf,
        jpg,
    }
}

pub fn scan(root: &Path) -> io::Result<Scan> {
    if !std::fs::metadata(root)?.is_dir() {
        return Err(io::Error::new(io::ErrorKind::NotADirectory, "not a folder"));
    }
    std::fs::read_dir(root)?;
    type Group = (Vec<FileStamp>, Vec<FileStamp>, String);
    let mut groups: BTreeMap<(PathBuf, String), Group> = BTreeMap::new();
    let mut errors = 0;
    let walker = WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry.file_type().is_dir()
                || !is_skipped_dir(&entry.file_name().to_string_lossy())
        });
    for entry in walker {
        let Ok(entry) = entry else {
            errors += 1;
            continue;
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let Some(kind) = kind_of(path) else { continue };
        let (Some(dir), Some(stem)) = (path.parent(), path.file_stem()) else {
            continue;
        };
        let Some(file) = stamp(&entry) else {
            errors += 1;
            continue;
        };
        let stem = stem.to_string_lossy().into_owned();
        let group = groups
            .entry((dir.to_path_buf(), stem.to_lowercase()))
            .or_insert_with(|| (Vec::new(), Vec::new(), stem));
        match kind {
            Kind::Raf => group.0.push(file),
            Kind::Jpg => group.1.push(file),
        }
    }
    let mut sources = Vec::new();
    for ((dir, _), (mut rafs, mut jpgs, stem)) in groups {
        rafs.sort_by(|a, b| a.path.cmp(&b.path));
        jpgs.sort_by_key(jpg_rank);
        let mut rafs = rafs.into_iter();
        let mut jpgs = jpgs.into_iter();
        sources.push(Source {
            dir: dir.clone(),
            stem: stem.clone(),
            raf: rafs.next(),
            jpg: jpgs.next(),
        });
        sources.extend(rafs.map(|file| single(&dir, &stem, Kind::Raf, file)));
        sources.extend(jpgs.map(|file| single(&dir, &stem, Kind::Jpg, file)));
    }
    Ok(Scan { sources, errors })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, b"x")
    }

    #[test]
    fn pairs_raf_and_jpg_by_stem_and_skips_other_files() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        for name in [
            "DSCF0001.RAF",
            "DSCF0001.JPG",
            "DSCF0002.raf",
            "DSCF0003.jpeg",
            "notes.txt",
            "sub/DSCF0004.JPG",
            "_to_delete/DSCF0005.RAF",
            ".cache/DSCF0006.JPG",
        ] {
            touch(&root.join(name))?;
        }
        let sources = scan(root)?.sources;
        let summary: Vec<(String, bool, bool)> = sources
            .iter()
            .map(|s| (s.stem.clone(), s.raf.is_some(), s.jpg.is_some()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("DSCF0001".into(), true, true),
                ("DSCF0002".into(), true, false),
                ("DSCF0003".into(), false, true),
                ("DSCF0004".into(), false, true),
            ]
        );
        Ok(())
    }

    #[test]
    fn second_file_with_the_same_stem_becomes_its_own_photo() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        touch(&dir.path().join("A.jpg"))?;
        touch(&dir.path().join("A.jpeg"))?;
        touch(&dir.path().join("A.RAF"))?;
        let sources = scan(dir.path())?.sources;
        assert_eq!(sources.len(), 2);
        assert_eq!(sources.iter().filter(|s| s.raf.is_some()).count(), 1);
        assert_eq!(sources.iter().filter(|s| s.jpg.is_some()).count(), 2);
        Ok(())
    }

    #[test]
    fn display_file_prefers_jpg() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        touch(&dir.path().join("A.RAF"))?;
        touch(&dir.path().join("A.JPG"))?;
        let sources = scan(dir.path())?.sources;
        let display = sources.first().and_then(Source::display_file);
        assert_eq!(display.map(|f| kind_of(&f.path)), Some(Some(Kind::Jpg)));
        Ok(())
    }

    #[test]
    fn id_follows_the_anchor_file_not_its_siblings() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        touch(&dir.path().join("A.RAF"))?;
        touch(&dir.path().join("A.JPG"))?;
        let pair = scan(dir.path())?.sources.remove(0).id();
        touch(&dir.path().join("A.jpeg"))?;
        let sources = scan(dir.path())?.sources;
        assert!(
            sources.iter().any(
                |s| s.id() == pair && s.jpg.as_ref().is_some_and(|f| f.path.ends_with("A.JPG"))
            )
        );
        let extra = sources.iter().find(|s| s.id() != pair).map(Source::id);
        std::fs::remove_file(dir.path().join("A.RAF"))?;
        std::fs::remove_file(dir.path().join("A.JPG"))?;
        let left = scan(dir.path())?.sources;
        assert_eq!(left.len(), 1);
        assert_ne!(left.first().map(Source::id), Some(pair));
        assert_eq!(left.first().map(Source::id), extra);
        Ok(())
    }

    #[test]
    fn missing_root_is_an_error_not_an_empty_folder() {
        assert!(scan(Path::new("/definitely/not/here")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinked_directories() -> std::io::Result<()> {
        let outside = tempfile::tempdir()?;
        touch(&outside.path().join("secret.JPG"))?;
        let dir = tempfile::tempdir()?;
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link"))?;
        assert!(scan(dir.path())?.sources.is_empty());
        Ok(())
    }
}
