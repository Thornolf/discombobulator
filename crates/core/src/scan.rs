use std::collections::BTreeMap;
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
        photo_id(&self.dir, &self.stem)
    }

    pub fn size(&self) -> u64 {
        self.files().map(|file| file.size).sum()
    }

    pub fn modified_ms(&self) -> i64 {
        self.files().map(|file| file.modified_ms).max().unwrap_or(0)
    }
}

pub fn photo_id(dir: &Path, stem: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(dir.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(stem.to_lowercase().as_bytes());
    hasher.finalize().to_hex()[..20].to_owned()
}

fn is_skipped_dir(name: &str) -> bool {
    name.starts_with('.') || name.eq_ignore_ascii_case(TRASH_DIR)
}

pub fn scan(root: &Path) -> Vec<Source> {
    let mut sources: BTreeMap<(PathBuf, String), Source> = BTreeMap::new();
    let walker = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry.file_type().is_dir()
                || !is_skipped_dir(&entry.file_name().to_string_lossy())
        });
    for entry in walker.filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let Some(kind) = kind_of(path) else { continue };
        let (Some(dir), Some(stem)) = (path.parent(), path.file_stem()) else {
            continue;
        };
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let modified_ms = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .and_then(|duration| i64::try_from(duration.as_millis()).ok())
            .unwrap_or(0);
        let stamp = FileStamp {
            path: path.to_path_buf(),
            size: metadata.len(),
            modified_ms,
        };
        let stem = stem.to_string_lossy().into_owned();
        let source = sources
            .entry((dir.to_path_buf(), stem.to_lowercase()))
            .or_insert_with(|| Source {
                dir: dir.to_path_buf(),
                stem: stem.clone(),
                raf: None,
                jpg: None,
            });
        let slot = match kind {
            Kind::Raf => &mut source.raf,
            Kind::Jpg => &mut source.jpg,
        };
        if slot
            .as_ref()
            .is_none_or(|existing| existing.path > stamp.path)
        {
            *slot = Some(stamp);
        }
    }
    sources.into_values().collect()
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
        let sources = scan(root);
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
    fn display_file_prefers_jpg() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        touch(&dir.path().join("A.RAF"))?;
        touch(&dir.path().join("A.JPG"))?;
        let sources = scan(dir.path());
        let display = sources.first().and_then(Source::display_file);
        assert_eq!(display.map(|f| kind_of(&f.path)), Some(Some(Kind::Jpg)));
        Ok(())
    }

    #[test]
    fn id_is_stable_and_case_insensitive_on_stem() {
        let dir = Path::new("/photos/2026");
        assert_eq!(photo_id(dir, "DSCF0001"), photo_id(dir, "dscf0001"));
        assert_ne!(photo_id(dir, "DSCF0001"), photo_id(dir, "DSCF0002"));
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinked_directories() -> std::io::Result<()> {
        let outside = tempfile::tempdir()?;
        touch(&outside.path().join("secret.JPG"))?;
        let dir = tempfile::tempdir()?;
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link"))?;
        assert!(scan(dir.path()).is_empty());
        Ok(())
    }
}
