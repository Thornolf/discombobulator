use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension, Row, params};
use thiserror::Error;

use crate::model::{Decision, Mark, Photo};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("database lock poisoned")]
    Poisoned,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHashes {
    pub quick: String,
    pub full: Option<String>,
}

pub struct Store {
    connection: Mutex<Connection>,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS photos (
    id TEXT PRIMARY KEY,
    dir TEXT NOT NULL,
    stem TEXT NOT NULL,
    raf TEXT,
    jpg TEXT,
    size INTEGER NOT NULL,
    modified_ms INTEGER NOT NULL,
    fingerprint TEXT NOT NULL,
    processed_fingerprint TEXT,
    capture_ms INTEGER,
    orientation INTEGER NOT NULL DEFAULT 1,
    dhash INTEGER,
    camera_rating INTEGER,
    width INTEGER,
    height INTEGER,
    error TEXT
);
CREATE INDEX IF NOT EXISTS photos_dir ON photos(dir);
CREATE TABLE IF NOT EXISTS decisions (
    id TEXT PRIMARY KEY,
    mark TEXT,
    favorite INTEGER NOT NULL DEFAULT 0,
    rating INTEGER NOT NULL DEFAULT 0,
    updated_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS sidecars (
    id TEXT PRIMARY KEY,
    fields TEXT NOT NULL
);
";

const PHOTO_COLUMNS: &str = "p.id, p.dir, p.stem, p.raf, p.jpg, p.size, p.modified_ms, p.fingerprint,
    p.processed_fingerprint, p.capture_ms, p.orientation, p.dhash, p.camera_rating, p.width, p.height,
    p.error, d.mark, d.favorite, d.rating, d.id IS NOT NULL";

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn photo_from_row(row: &Row<'_>) -> rusqlite::Result<Photo> {
    let optional_path = |value: Option<String>| value.map(PathBuf::from);
    let mark: Option<String> = row.get(16)?;
    let favorite: Option<bool> = row.get(17)?;
    let rating: Option<u8> = row.get(18)?;
    Ok(Photo {
        id: row.get(0)?,
        dir: PathBuf::from(row.get::<_, String>(1)?),
        stem: row.get(2)?,
        raf: optional_path(row.get(3)?),
        jpg: optional_path(row.get(4)?),
        size: row.get::<_, i64>(5)?.max(0) as u64,
        modified_ms: row.get(6)?,
        fingerprint: row.get(7)?,
        processed_fingerprint: row.get(8)?,
        capture_ms: row.get(9)?,
        orientation: row.get(10)?,
        dhash: row.get::<_, Option<i64>>(11)?.map(|value| value as u64),
        camera_rating: row.get(12)?,
        width: row.get(13)?,
        height: row.get(14)?,
        error: row.get(15)?,
        decision: Decision {
            mark: mark.as_deref().and_then(Mark::parse),
            favorite: favorite.unwrap_or(false),
            rating: rating.unwrap_or(0),
        },
        has_decision: row.get(19)?,
    })
}

fn successor<'a>(
    vanished: &Photo,
    photos: &'a [Photo],
    known: &std::collections::HashSet<String>,
) -> Option<&'a Photo> {
    let old_files: Vec<&Path> = vanished
        .raf
        .iter()
        .chain(vanished.jpg.iter())
        .map(PathBuf::as_path)
        .collect();
    let mut candidates = photos.iter().filter(|photo| {
        !known.contains(&photo.id)
            && photo
                .raf
                .iter()
                .chain(photo.jpg.iter())
                .any(|file| old_files.contains(&file.as_path()))
    });
    let first = candidates.next()?;
    candidates.next().is_none().then_some(first)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendering {
    pub fingerprint: String,
    pub capture_ms: Option<i64>,
    pub orientation: u16,
    pub dhash: Option<u64>,
    pub camera_rating: Option<u8>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub error: Option<String>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Store, StoreError> {
        Store::init(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Store, StoreError> {
        Store::init(Connection::open_in_memory()?)
    }

    fn init(connection: Connection) -> Result<Store, StoreError> {
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version < 1 {
            connection.execute_batch(SCHEMA)?;
            connection.pragma_update(None, "user_version", 1)?;
        }
        if version < 2 {
            connection.execute_batch(
                "CREATE TABLE IF NOT EXISTS hashes (
                    id TEXT PRIMARY KEY,
                    fingerprint TEXT NOT NULL,
                    quick TEXT NOT NULL,
                    full TEXT
                );",
            )?;
            connection.pragma_update(None, "user_version", 2)?;
        }
        Ok(Store {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>, StoreError> {
        self.connection.lock().map_err(|_| StoreError::Poisoned)
    }

    pub fn photos_under(&self, root: &Path) -> Result<Vec<Photo>, StoreError> {
        let connection = self.lock()?;
        let root_text = path_text(root);
        let separator = std::path::MAIN_SEPARATOR.to_string();
        let prefix = if root_text.ends_with(&separator) {
            root_text.clone()
        } else {
            format!("{root_text}{separator}")
        };
        let mut statement = connection.prepare(&format!(
            "SELECT {PHOTO_COLUMNS} FROM photos p LEFT JOIN decisions d ON d.id = p.id
             WHERE p.dir = ?1 OR substr(p.dir, 1, length(?2)) = ?2"
        ))?;
        let rows = statement.query_map(params![root_text, prefix], photo_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn photo(&self, id: &str) -> Result<Option<Photo>, StoreError> {
        let connection = self.lock()?;
        Ok(connection
            .query_row(
                &format!(
                    "SELECT {PHOTO_COLUMNS} FROM photos p LEFT JOIN decisions d ON d.id = p.id WHERE p.id = ?1"
                ),
                params![id],
                photo_from_row,
            )
            .optional()?)
    }

    pub fn replace_photos(
        &self,
        root: &Path,
        photos: &[Photo],
        unreadable: &[PathBuf],
    ) -> Result<(), StoreError> {
        let all_existing = self.photos_under(root)?;
        let known: std::collections::HashSet<String> =
            all_existing.iter().map(|photo| photo.id.clone()).collect();
        let existing: Vec<Photo> = all_existing
            .into_iter()
            .filter(|photo| !unreadable.iter().any(|dir| photo.dir.starts_with(dir)))
            .collect();
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        {
            let keep: std::collections::HashSet<&str> =
                photos.iter().map(|p| p.id.as_str()).collect();
            let mut delete = transaction.prepare("DELETE FROM photos WHERE id = ?1")?;
            let mut carry = transaction.prepare(
                "UPDATE decisions SET id = ?2 WHERE id = ?1
                 AND NOT EXISTS (SELECT 1 FROM decisions WHERE id = ?2)",
            )?;
            let mut forget = transaction.prepare("DELETE FROM decisions WHERE id = ?1")?;
            let mut unhash = transaction.prepare("DELETE FROM hashes WHERE id = ?1")?;
            for vanished in existing.iter().filter(|p| !keep.contains(p.id.as_str())) {
                delete.execute(params![vanished.id])?;
                if let Some(successor) = successor(vanished, photos, &known) {
                    carry.execute(params![vanished.id, successor.id])?;
                }
                forget.execute(params![vanished.id])?;
                unhash.execute(params![vanished.id])?;
            }
            let mut upsert = transaction.prepare(
                "INSERT INTO photos (id, dir, stem, raf, jpg, size, modified_ms, fingerprint)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET dir = excluded.dir, stem = excluded.stem, raf = excluded.raf,
                     jpg = excluded.jpg, size = excluded.size, modified_ms = excluded.modified_ms,
                     fingerprint = excluded.fingerprint",
            )?;
            for photo in photos {
                upsert.execute(params![
                    photo.id,
                    path_text(&photo.dir),
                    photo.stem,
                    photo.raf.as_deref().map(path_text),
                    photo.jpg.as_deref().map(path_text),
                    i64::try_from(photo.size).unwrap_or(i64::MAX),
                    photo.modified_ms,
                    photo.fingerprint,
                ])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn save_rendering(&self, id: &str, rendering: &Rendering) -> Result<(), StoreError> {
        self.lock()?.execute(
            "UPDATE photos SET processed_fingerprint = ?2, capture_ms = ?3, orientation = ?4, dhash = ?5,
                camera_rating = ?6, width = ?7, height = ?8, error = ?9 WHERE id = ?1",
            params![
                id,
                rendering.fingerprint,
                rendering.capture_ms,
                rendering.orientation,
                rendering.dhash.map(|value| value as i64),
                rendering.camera_rating,
                rendering.width,
                rendering.height,
                rendering.error,
            ],
        )?;
        Ok(())
    }

    pub fn set_decision(
        &self,
        id: &str,
        decision: Decision,
        now_ms: i64,
    ) -> Result<(), StoreError> {
        self.lock()?.execute(
            "INSERT INTO decisions (id, mark, favorite, rating, updated_ms) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET mark = excluded.mark, favorite = excluded.favorite,
                 rating = excluded.rating, updated_ms = excluded.updated_ms",
            params![
                id,
                decision.mark.map(Mark::as_str),
                decision.favorite,
                decision.rating,
                now_ms
            ],
        )?;
        Ok(())
    }

    pub fn hashes(&self, id: &str, fingerprint: &str) -> Result<Option<FileHashes>, StoreError> {
        Ok(self
            .lock()?
            .query_row(
                "SELECT quick, full FROM hashes WHERE id = ?1 AND fingerprint = ?2",
                params![id, fingerprint],
                |row| {
                    Ok(FileHashes {
                        quick: row.get(0)?,
                        full: row.get(1)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn save_hashes(
        &self,
        id: &str,
        fingerprint: &str,
        hashes: &FileHashes,
    ) -> Result<(), StoreError> {
        self.lock()?.execute(
            "INSERT INTO hashes (id, fingerprint, quick, full) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET fingerprint = excluded.fingerprint, quick = excluded.quick,
                 full = excluded.full",
            params![id, fingerprint, hashes.quick, hashes.full],
        )?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>, StoreError> {
        Ok(self
            .lock()?
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), StoreError> {
        self.lock()?.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn sidecar_fields(&self, id: &str) -> Result<Vec<String>, StoreError> {
        let fields: Option<String> = self
            .lock()?
            .query_row(
                "SELECT fields FROM sidecars WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(fields
            .unwrap_or_default()
            .split('\n')
            .filter(|field| !field.is_empty())
            .map(str::to_owned)
            .collect())
    }

    pub fn set_sidecar_fields(&self, id: &str, fields: &[String]) -> Result<(), StoreError> {
        self.lock()?.execute(
            "INSERT INTO sidecars (id, fields) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET fields = excluded.fields",
            params![id, fields.join("\n")],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_sidecar_fields() -> Result<(), StoreError> {
        let store = Store::in_memory()?;
        assert!(store.sidecar_fields("a")?.is_empty());
        let fields = vec!["xmp:Rating".to_owned(), "xmpDM:pick".to_owned()];
        store.set_sidecar_fields("a", &fields)?;
        assert_eq!(store.sidecar_fields("a")?, fields);
        store.set_sidecar_fields("a", &[])?;
        assert!(store.sidecar_fields("a")?.is_empty());
        Ok(())
    }

    fn photo(dir: &str, stem: &str) -> Photo {
        let dir = PathBuf::from(dir);
        Photo {
            id: crate::scan::photo_id(&dir, &format!("{stem}.JPG")),
            jpg: Some(dir.join(format!("{stem}.JPG"))),
            dir,
            stem: stem.to_owned(),
            raf: None,
            size: 10,
            modified_ms: 5,
            fingerprint: "f1".into(),
            processed_fingerprint: None,
            capture_ms: None,
            orientation: 1,
            dhash: None,
            camera_rating: None,
            width: None,
            height: None,
            error: None,
            decision: Decision::default(),
            has_decision: false,
        }
    }

    #[test]
    fn removed_photo_forgets_its_decision() -> Result<(), StoreError> {
        let store = Store::in_memory()?;
        let p = photo("/p", "A");
        store.replace_photos(Path::new("/p"), std::slice::from_ref(&p), &[])?;
        let delete = Decision {
            mark: Some(Mark::Delete),
            favorite: false,
            rating: 0,
        };
        store.set_decision(&p.id, delete, 1)?;
        store.replace_photos(Path::new("/p"), &[], &[])?;
        store.replace_photos(Path::new("/p"), std::slice::from_ref(&p), &[])?;
        assert_eq!(
            store.photo(&p.id)?.map(|p| p.decision),
            Some(Decision::default())
        );
        Ok(())
    }

    fn delete_mark() -> Decision {
        Decision {
            mark: Some(Mark::Delete),
            favorite: true,
            rating: 2,
        }
    }

    fn with_raf(jpg_only: &Photo) -> Photo {
        let raf = jpg_only.dir.join(format!("{}.RAF", jpg_only.stem));
        Photo {
            id: crate::scan::photo_id(&jpg_only.dir, &format!("{}.RAF", jpg_only.stem)),
            raf: Some(raf),
            ..jpg_only.clone()
        }
    }

    #[test]
    fn decision_follows_a_jpg_when_its_raf_appears() -> Result<(), StoreError> {
        let store = Store::in_memory()?;
        let jpg_only = photo("/p", "A");
        store.replace_photos(Path::new("/p"), std::slice::from_ref(&jpg_only), &[])?;
        store.set_decision(&jpg_only.id, delete_mark(), 1)?;
        let pair = with_raf(&jpg_only);
        store.replace_photos(Path::new("/p"), std::slice::from_ref(&pair), &[])?;
        assert!(store.photo(&jpg_only.id)?.is_none());
        assert_eq!(
            store.photo(&pair.id)?.map(|p| p.decision),
            Some(delete_mark())
        );
        Ok(())
    }

    #[test]
    fn decision_follows_a_pair_when_its_raf_disappears() -> Result<(), StoreError> {
        let store = Store::in_memory()?;
        let jpg_only = photo("/p", "A");
        let pair = with_raf(&jpg_only);
        store.replace_photos(Path::new("/p"), std::slice::from_ref(&pair), &[])?;
        store.set_decision(&pair.id, delete_mark(), 1)?;
        store.replace_photos(Path::new("/p"), std::slice::from_ref(&jpg_only), &[])?;
        assert_eq!(
            store.photo(&jpg_only.id)?.map(|p| p.decision),
            Some(delete_mark())
        );
        Ok(())
    }

    #[test]
    fn decision_never_moves_onto_a_photo_that_already_existed() -> Result<(), StoreError> {
        let store = Store::in_memory()?;
        let dir = PathBuf::from("/p");
        let pair = Photo {
            id: crate::scan::photo_id(&dir, "A.RAF"),
            raf: Some(dir.join("A.RAF")),
            jpg: Some(dir.join("A.JPG")),
            ..photo("/p", "A")
        };
        let extra = Photo {
            id: crate::scan::photo_id(&dir, "A.jpeg"),
            jpg: Some(dir.join("A.jpeg")),
            ..photo("/p", "A")
        };
        store.replace_photos(&dir, &[pair.clone(), extra.clone()], &[])?;
        store.set_decision(&extra.id, delete_mark(), 1)?;
        let repaired = Photo {
            jpg: Some(dir.join("A.jpeg")),
            ..pair.clone()
        };
        store.replace_photos(&dir, std::slice::from_ref(&repaired), &[])?;
        assert_eq!(store.photo(&pair.id)?.map(|p| p.has_decision), Some(false));
        Ok(())
    }

    #[test]
    fn decision_never_moves_to_a_photo_without_a_shared_file() -> Result<(), StoreError> {
        let store = Store::in_memory()?;
        let a = photo("/p", "A");
        let b = photo("/p", "B");
        store.replace_photos(Path::new("/p"), std::slice::from_ref(&a), &[])?;
        store.set_decision(&a.id, delete_mark(), 1)?;
        store.replace_photos(Path::new("/p"), std::slice::from_ref(&b), &[])?;
        assert_eq!(store.photo(&b.id)?.map(|p| p.has_decision), Some(false));
        store.replace_photos(Path::new("/p"), std::slice::from_ref(&a), &[])?;
        assert_eq!(store.photo(&a.id)?.map(|p| p.has_decision), Some(false));
        Ok(())
    }

    #[test]
    fn carried_decision_does_not_overwrite_an_existing_one() -> Result<(), StoreError> {
        let store = Store::in_memory()?;
        let jpg_only = photo("/p", "A");
        let pair = with_raf(&jpg_only);
        store.replace_photos(Path::new("/p"), std::slice::from_ref(&jpg_only), &[])?;
        store.set_decision(&jpg_only.id, delete_mark(), 1)?;
        store.set_decision(&pair.id, Decision::default(), 2)?;
        store.replace_photos(Path::new("/p"), std::slice::from_ref(&pair), &[])?;
        assert_eq!(
            store.photo(&pair.id)?.map(|p| p.decision),
            Some(Decision::default())
        );
        Ok(())
    }

    #[test]
    fn unreadable_folders_are_not_pruned() -> Result<(), StoreError> {
        let store = Store::in_memory()?;
        let root = PathBuf::from("/p");
        let locked_dir = root.join("locked");
        let locked = photo(&locked_dir.to_string_lossy(), "A");
        let open = photo(&root.join("open").to_string_lossy(), "B");
        store.replace_photos(&root, &[locked.clone(), open.clone()], &[])?;
        store.replace_photos(&root, &[], &[locked_dir])?;
        assert!(store.photo(&locked.id)?.is_some());
        assert!(store.photo(&open.id)?.is_none());
        Ok(())
    }

    #[test]
    fn replaces_photos_under_root_only() -> Result<(), StoreError> {
        let store = Store::in_memory()?;
        let sep = std::path::MAIN_SEPARATOR;
        let root = format!("{sep}photos");
        let sub = format!("{root}{sep}a");
        let other = format!("{sep}photos2");
        store.replace_photos(
            Path::new(&root),
            &[photo(&root, "A"), photo(&sub, "B")],
            &[],
        )?;
        store.replace_photos(Path::new(&other), &[photo(&other, "C")], &[])?;
        store.replace_photos(Path::new(&root), &[photo(&sub, "B")], &[])?;
        assert_eq!(store.photos_under(Path::new(&root))?.len(), 1);
        assert_eq!(store.photos_under(Path::new(&other))?.len(), 1);
        Ok(())
    }

    #[test]
    fn decisions_round_trip_and_cleared_decision_is_kept() -> Result<(), StoreError> {
        let store = Store::in_memory()?;
        let p = photo("/p", "A");
        store.replace_photos(Path::new("/p"), std::slice::from_ref(&p), &[])?;
        let decision = Decision {
            mark: Some(Mark::Unsure),
            favorite: true,
            rating: 4,
        };
        store.set_decision(&p.id, decision, 1)?;
        assert_eq!(store.photo(&p.id)?.map(|p| p.decision), Some(decision));
        store.set_decision(&p.id, Decision::default(), 2)?;
        let cleared = store.photo(&p.id)?;
        assert_eq!(
            cleared.as_ref().map(|p| p.decision),
            Some(Decision::default())
        );
        assert_eq!(cleared.map(|p| p.has_decision), Some(true));
        Ok(())
    }

    #[test]
    fn rendering_marks_photo_processed_for_its_fingerprint() -> Result<(), StoreError> {
        let store = Store::in_memory()?;
        let p = photo("/p", "A");
        store.replace_photos(Path::new("/p"), std::slice::from_ref(&p), &[])?;
        store.save_rendering(
            &p.id,
            &Rendering {
                fingerprint: "f1".into(),
                capture_ms: Some(42),
                orientation: 6,
                dhash: Some(u64::MAX),
                camera_rating: Some(3),
                width: Some(10),
                height: Some(20),
                error: None,
            },
        )?;
        let stored = store.photo(&p.id)?;
        assert_eq!(stored.as_ref().map(Photo::is_processed), Some(true));
        assert_eq!(stored.and_then(|p| p.dhash), Some(u64::MAX));
        Ok(())
    }

    #[test]
    fn hashes_are_tied_to_the_fingerprint() -> Result<(), StoreError> {
        let store = Store::in_memory()?;
        let hashes = FileHashes {
            quick: "q".into(),
            full: Some("f".into()),
        };
        store.save_hashes("id", "v1", &hashes)?;
        assert_eq!(store.hashes("id", "v1")?, Some(hashes));
        assert_eq!(store.hashes("id", "v2")?, None);
        Ok(())
    }

    #[test]
    fn removed_photo_forgets_its_hashes() -> Result<(), StoreError> {
        let store = Store::in_memory()?;
        let p = photo("/p", "A");
        store.replace_photos(Path::new("/p"), std::slice::from_ref(&p), &[])?;
        let hashes = FileHashes {
            quick: "q".into(),
            full: None,
        };
        store.save_hashes(&p.id, &p.fingerprint, &hashes)?;
        store.replace_photos(Path::new("/p"), std::slice::from_ref(&p), &[])?;
        assert_eq!(store.hashes(&p.id, &p.fingerprint)?, Some(hashes));
        store.replace_photos(Path::new("/p"), &[], &[])?;
        assert_eq!(store.hashes(&p.id, &p.fingerprint)?, None);
        Ok(())
    }

    #[test]
    fn settings_round_trip() -> Result<(), StoreError> {
        let store = Store::in_memory()?;
        assert_eq!(store.setting("k")?, None);
        store.set_setting("k", "v")?;
        store.set_setting("k", "w")?;
        assert_eq!(store.setting("k")?, Some("w".into()));
        Ok(())
    }
}
