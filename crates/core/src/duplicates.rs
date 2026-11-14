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
    let same_size: Vec<&Photo> = groups_by(sized).into_iter().flatten().collect();
    let quick: Vec<(String, &Photo)> = same_size
        .par_iter()
        .filter_map(|photo| hashes_for(library, photo, false).map(|h| (h.quick, *photo)))
        .collect();
    let same_quick: Vec<&Photo> = groups_by(quick).into_iter().flatten().collect();
    let full: Vec<(String, &Photo)> = same_quick
        .par_iter()
        .filter_map(|photo| {
            hashes_for(library, photo, true).and_then(|h| h.full.map(|full| (full, *photo)))
        })
        .collect();
    groups_by(full)
        .into_iter()
        .map(|same| same.into_iter().map(|photo| photo.id.clone()).collect())
        .collect()
}

const EDGES: usize = 65;
const NO_CHILD: u32 = u32::MAX;

pub struct BkTree {
    nodes: Vec<(u64, usize, [u32; EDGES])>,
}

impl BkTree {
    pub fn new() -> BkTree {
        BkTree { nodes: Vec::new() }
    }

    pub fn insert(&mut self, hash: u64, item: usize) {
        let index = self.nodes.len();
        if index == 0 {
            self.nodes.push((hash, item, [NO_CHILD; EDGES]));
            return;
        }
        let mut current = 0;
        loop {
            let distance = hamming(self.nodes[current].0, hash) as usize;
            let next = self.nodes[current].2[distance];
            if next != NO_CHILD {
                current = next as usize;
                continue;
            }
            let Ok(child) = u32::try_from(index) else {
                return;
            };
            self.nodes.push((hash, item, [NO_CHILD; EDGES]));
            self.nodes[current].2[distance] = child;
            return;
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
            let low = distance.saturating_sub(tolerance) as usize;
            let high = ((distance + tolerance) as usize).min(EDGES - 1);
            pending.extend(
                children[low..=high]
                    .iter()
                    .filter(|&&child| child != NO_CHILD)
                    .map(|&child| child as usize),
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

pub fn similar_groups(hashes: &[(String, u64)], tolerance: u32) -> Vec<Vec<String>> {
    let tolerance = tolerance.min(MAX_TOLERANCE);
    let mut tree = BkTree::new();
    for (index, (_, hash)) in hashes.iter().enumerate() {
        tree.insert(*hash, index);
    }
    let mut assigned = vec![false; hashes.len()];
    let mut groups = Vec::new();
    for (seed, (_, hash)) in hashes.iter().enumerate() {
        if assigned[seed] {
            continue;
        }
        let mut members: Vec<usize> = tree
            .within(*hash, tolerance)
            .into_iter()
            .filter(|&other| !assigned[other])
            .collect();
        if members.len() < 2 {
            continue;
        }
        members.sort_unstable();
        for &member in &members {
            assigned[member] = true;
        }
        groups.push(
            members
                .into_iter()
                .map(|index| hashes[index].0.clone())
                .collect(),
        );
    }
    groups
}

pub fn find_duplicates(library: &Library, tolerance: u32) -> Result<Vec<Group>, LibraryError> {
    let exact = exact_groups(library, &library.photos()?);
    combine(library, exact, tolerance)
}

pub fn combine(
    library: &Library,
    exact: Vec<Vec<String>>,
    tolerance: u32,
) -> Result<Vec<Group>, LibraryError> {
    let photos = library.photos()?;
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
    fn similar_groups_form_around_a_seed_without_chaining() {
        let hashes = vec![
            ("a".to_owned(), 0b0000u64),
            ("b".to_owned(), 0b0011),
            ("c".to_owned(), 0b1111),
            ("d".to_owned(), 0b0011_1111),
            ("e".to_owned(), u64::MAX),
        ];
        assert_eq!(
            similar_groups(&hashes, 2),
            vec![
                vec!["a".to_owned(), "b".to_owned()],
                vec!["c".to_owned(), "d".to_owned()]
            ]
        );
        assert!(similar_groups(&hashes, 0).is_empty());
    }

    #[test]
    fn identical_hashes_group_at_distance_zero() {
        let hashes = vec![
            ("a".to_owned(), 0xABCDu64),
            ("b".to_owned(), 0xABCD),
            ("c".to_owned(), 0xABCD),
            ("d".to_owned(), 0xABCC),
        ];
        assert_eq!(
            similar_groups(&hashes, 0),
            vec![vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]]
        );
        let mut tree = BkTree::new();
        for (index, (_, hash)) in hashes.iter().enumerate() {
            tree.insert(*hash, index);
        }
        let mut exact = tree.within(0xABCD, 0);
        exact.sort_unstable();
        assert_eq!(exact, vec![0, 1, 2]);
        let mut all = tree.within(0xABCD, MAX_TOLERANCE);
        all.sort_unstable();
        assert_eq!(all, vec![0, 1, 2, 3]);
    }

    #[test]
    fn changed_file_is_hashed_again() -> Result<(), LibraryError> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("photos");
        std::fs::create_dir_all(root.join("backup"))?;
        let mut bytes = vec![7u8; 200_000];
        std::fs::write(root.join("A.JPG"), &bytes)?;
        std::fs::write(root.join("backup").join("A.JPG"), &bytes)?;
        let store = Arc::new(Store::in_memory()?);
        let cache = Arc::new(Cache::new(dir.path().join("cache"))?);
        let library = Library::open(&root, store, cache)?;
        assert_eq!(exact_groups(&library, &library.photos()?).len(), 1);
        bytes[100_000] = 8;
        let copy = root.join("backup").join("A.JPG");
        std::fs::write(&copy, &bytes)?;
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&copy)?
            .set_modified(later)?;
        library.rescan()?;
        assert!(exact_groups(&library, &library.photos()?).is_empty());
        Ok(())
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
                        .strip_prefix(library.root())
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
