use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use rayon::prelude::*;
use serde::Serialize;

use crate::fsx;
use crate::imaging::hamming;
use crate::library::{Library, LibraryError};
use crate::model::Photo;
use crate::store::FileHashes;

const EDGE_BYTES: u64 = 64 * 1024;
pub const MAX_TOLERANCE: u32 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Exact,
    Similar,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Group {
    pub kind: Kind,
    pub ids: Vec<String>,
}

fn anchor(photo: &Photo) -> Option<&Path> {
    photo.raf.as_deref().or(photo.jpg.as_deref())
}

pub fn quick_hash(path: &Path) -> std::io::Result<String> {
    let mut file = fsx::open_regular(path)?;
    let length = file.metadata()?.len();
    let mut hasher = blake3::Hasher::new();
    hasher.update(&length.to_le_bytes());
    let mut head = Vec::new();
    (&mut file).take(EDGE_BYTES).read_to_end(&mut head)?;
    hasher.update(&head);
    if length > 2 * EDGE_BYTES {
        file.seek(SeekFrom::Start(length - EDGE_BYTES))?;
        let mut tail = Vec::new();
        file.take(EDGE_BYTES).read_to_end(&mut tail)?;
        hasher.update(&tail);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

pub fn full_hash(path: &Path) -> std::io::Result<String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(fsx::open_regular(path)?)?;
    Ok(hasher.finalize().to_hex().to_string())
}

fn file_len(photo: &Photo) -> Option<u64> {
    std::fs::symlink_metadata(anchor(photo)?)
        .ok()
        .filter(|metadata| metadata.file_type().is_file())
        .map(|metadata| metadata.len())
}

fn hashes_for(library: &Library, photo: &Photo, need_full: bool) -> Option<FileHashes> {
    let store = library.store();
    let cached = store.hashes(&photo.id, &photo.fingerprint).ok().flatten();
    if let Some(hashes) = &cached
        && (!need_full || hashes.full.is_some())
    {
        return cached;
    }
    let path = anchor(photo)?;
    let quick = match cached {
        Some(hashes) => hashes.quick,
        None => quick_hash(path).ok()?,
    };
    let full = if need_full {
        Some(full_hash(path).ok()?)
    } else {
        None
    };
    let hashes = FileHashes { quick, full };
    let _ = store.save_hashes(&photo.id, &photo.fingerprint, &hashes);
    Some(hashes)
}

fn groups_by<K: Ord, T>(items: Vec<(K, T)>) -> Vec<Vec<T>> {
    let mut map: BTreeMap<K, Vec<T>> = BTreeMap::new();
    for (key, item) in items {
        map.entry(key).or_default().push(item);
    }
    map.into_values().filter(|group| group.len() > 1).collect()
}

pub fn exact_groups(library: &Library, photos: &[Photo]) -> Vec<Vec<String>> {
    let sized: Vec<(u64, &Photo)> = photos
        .par_iter()
        .filter_map(|photo| file_len(photo).map(|len| (len, photo)))
        .collect();
    let mut result = Vec::new();
    for same_size in groups_by(sized) {
        let quick: Vec<(String, &Photo)> = same_size
            .par_iter()
            .filter_map(|photo| hashes_for(library, photo, false).map(|h| (h.quick, *photo)))
            .collect();
        for same_quick in groups_by(quick) {
            let full: Vec<(String, &Photo)> = same_quick
                .par_iter()
                .filter_map(|photo| {
                    hashes_for(library, photo, true).and_then(|h| h.full.map(|full| (full, *photo)))
                })
                .collect();
            for same in groups_by(full) {
                result.push(same.into_iter().map(|photo| photo.id.clone()).collect());
            }
        }
    }
    result
}

pub struct BkTree {
    nodes: Vec<(u64, usize, HashMap<u32, usize>)>,
}

impl BkTree {
    pub fn new() -> BkTree {
        BkTree { nodes: Vec::new() }
    }

    pub fn insert(&mut self, hash: u64, item: usize) {
        if self.nodes.is_empty() {
            self.nodes.push((hash, item, HashMap::new()));
            return;
        }
        let mut current = 0;
        loop {
            let distance = hamming(self.nodes[current].0, hash);
            match self.nodes[current].2.get(&distance) {
                Some(&next) => current = next,
                None => {
                    let index = self.nodes.len();
                    self.nodes.push((hash, item, HashMap::new()));
                    self.nodes[current].2.insert(distance, index);
                    return;
                }
            }
        }
    }

    pub fn within(&self, hash: u64, tolerance: u32) -> Vec<usize> {
        let mut found = Vec::new();
        if self.nodes.is_empty() {
            return found;
        }
        let mut pending = vec![0];
        while let Some(index) = pending.pop() {
            let (node_hash, item, children) = &self.nodes[index];
            let distance = hamming(*node_hash, hash);
            if distance <= tolerance {
                found.push(*item);
            }
            let low = distance.saturating_sub(tolerance);
            let high = distance + tolerance;
            pending.extend(
                children
                    .iter()
                    .filter(|(edge, _)| (low..=high).contains(*edge))
                    .map(|(_, child)| *child),
            );
        }
        found
    }
}

impl Default for BkTree {
    fn default() -> Self {
        BkTree::new()
    }
}

fn find(parents: &mut [usize], item: usize) -> usize {
    let mut root = item;
    while parents[root] != root {
        root = parents[root];
    }
    let mut current = item;
    while parents[current] != root {
        let next = parents[current];
        parents[current] = root;
        current = next;
    }
    root
}

pub fn similar_groups(hashes: &[(String, u64)], tolerance: u32) -> Vec<Vec<String>> {
    let tolerance = tolerance.min(MAX_TOLERANCE);
    let mut tree = BkTree::new();
    for (index, (_, hash)) in hashes.iter().enumerate() {
        tree.insert(*hash, index);
    }
    let mut parents: Vec<usize> = (0..hashes.len()).collect();
    for (index, (_, hash)) in hashes.iter().enumerate() {
        for other in tree.within(*hash, tolerance) {
            let (a, b) = (find(&mut parents, index), find(&mut parents, other));
            if a != b {
                parents[b] = a;
            }
        }
    }
    let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for index in 0..hashes.len() {
        let root = find(&mut parents, index);
        members.entry(root).or_default().push(index);
    }
    let mut groups: Vec<Vec<usize>> = members
        .into_values()
        .filter(|group| group.len() > 1)
        .collect();
    groups.sort_by_key(|group| group.first().copied());
    groups
        .into_iter()
        .map(|group| {
            group
                .into_iter()
                .map(|index| hashes[index].0.clone())
                .collect()
        })
        .collect()
}

pub fn find_duplicates(library: &Library, tolerance: u32) -> Result<Vec<Group>, LibraryError> {
    let photos = library.photos()?;
    let exact = exact_groups(library, &photos);
    let shadowed: std::collections::HashSet<&str> = exact
        .iter()
        .flat_map(|group| group.iter().skip(1).map(String::as_str))
        .collect();
    let candidates: Vec<(String, u64)> = photos
        .iter()
        .filter(|photo| !shadowed.contains(photo.id.as_str()))
        .filter_map(|photo| photo.dhash.map(|hash| (photo.id.clone(), hash)))
        .collect();
    let similar = similar_groups(&candidates, tolerance);
    let order: HashMap<&str, usize> = photos
        .iter()
        .enumerate()
        .map(|(index, photo)| (photo.id.as_str(), index))
        .collect();
    let position = |ids: &Vec<String>| {
        ids.iter()
            .filter_map(|id| order.get(id.as_str()).copied())
            .min()
            .unwrap_or(usize::MAX)
    };
    let mut groups: Vec<Group> = exact
        .into_iter()
        .map(|ids| Group {
            kind: Kind::Exact,
            ids,
        })
        .collect();
    groups.sort_by_key(|group| position(&group.ids));
    let mut similar: Vec<Group> = similar
        .into_iter()
        .map(|ids| Group {
            kind: Kind::Similar,
            ids,
        })
        .collect();
    similar.sort_by_key(|group| position(&group.ids));
    groups.extend(similar);
    for group in &mut groups {
        group
            .ids
            .sort_by_key(|id| order.get(id.as_str()).copied().unwrap_or(usize::MAX));
    }
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::imaging::fixtures::gradient_jpeg;
    use crate::library::Cache;
    use crate::store::Store;

    #[test]
    fn bk_tree_finds_everything_within_tolerance() {
        let mut tree = BkTree::new();
        let hashes = [0u64, 1, 3, 0xFF, u64::MAX, 0b1000_0000];
        for (index, hash) in hashes.iter().enumerate() {
            tree.insert(*hash, index);
        }
        let mut near = tree.within(0, 2);
        near.sort_unstable();
        assert_eq!(near, vec![0, 1, 2, 5]);
        assert_eq!(tree.within(u64::MAX, 0), vec![4]);
        assert!(BkTree::new().within(0, 64).is_empty());
    }

    #[test]
    fn similar_groups_are_transitive() {
        let hashes = vec![
            ("a".to_owned(), 0b0000u64),
            ("b".to_owned(), 0b0011),
            ("c".to_owned(), 0b1111),
            ("d".to_owned(), u64::MAX),
        ];
        assert_eq!(
            similar_groups(&hashes, 2),
            vec![vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]]
        );
        assert!(similar_groups(&hashes, 0).is_empty());
    }

    #[test]
    fn quick_hash_covers_length_head_and_tail() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        let mut bytes = vec![0u8; 300_000];
        std::fs::write(&a, &bytes)?;
        bytes[299_999] = 1;
        std::fs::write(&b, &bytes)?;
        assert_ne!(quick_hash(&a)?, quick_hash(&b)?);
        bytes[299_999] = 0;
        bytes[150_000] = 1;
        std::fs::write(&b, &bytes)?;
        assert_eq!(quick_hash(&a)?, quick_hash(&b)?);
        assert_ne!(full_hash(&a)?, full_hash(&b)?);
        Ok(())
    }

    #[test]
    fn finds_exact_copies_and_look_alikes() -> Result<(), LibraryError> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("photos");
        std::fs::create_dir_all(root.join("backup"))?;
        let original = gradient_jpeg(320, 240, 1);
        std::fs::write(root.join("A.JPG"), &original)?;
        std::fs::write(root.join("backup").join("A.JPG"), &original)?;
        std::fs::write(root.join("B.JPG"), gradient_jpeg(330, 240, 1))?;
        std::fs::write(root.join("C.JPG"), gradient_jpeg(320, 240, 120))?;
        let store = Arc::new(Store::in_memory()?);
        let cache = Arc::new(Cache::new(dir.path().join("cache"))?);
        let library = Library::open(&root, store, cache)?;
        library.process_all()?;
        let groups = find_duplicates(&library, 6)?;
        let names = |group: &Group| -> Vec<String> {
            group
                .ids
                .iter()
                .filter_map(|id| library.photo(id).ok().flatten())
                .map(|photo| {
                    photo
                        .dir
                        .join(photo.name())
                        .strip_prefix(&root)
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default()
                })
                .collect()
        };
        let exact: Vec<_> = groups.iter().filter(|g| g.kind == Kind::Exact).collect();
        assert_eq!(exact.len(), 1);
        let mut exact_names = names(exact[0]);
        exact_names.sort();
        assert_eq!(exact_names.len(), 2);
        assert!(exact_names.iter().all(|name| name.ends_with("A.JPG")));
        let similar: Vec<_> = groups.iter().filter(|g| g.kind == Kind::Similar).collect();
        assert_eq!(similar.len(), 1);
        let mut similar_names = names(similar[0]);
        similar_names.sort();
        assert_eq!(similar_names.len(), 2);
        assert!(similar_names.iter().any(|name| name == "B.JPG"));
        Ok(())
    }
}
