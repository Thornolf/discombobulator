use std::io::{Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::Path;

use thiserror::Error;

pub const MAGIC: &[u8] = b"FUJIFILMCCD-RAW ";
pub const HEADER_LEN: usize = 92;
const JPEG_OFFSET_AT: usize = 84;
const JPEG_LENGTH_AT: usize = 88;
pub const MAX_EMBEDDED_JPEG_LEN: u64 = 64 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum RafError {
    #[error("not a Fujifilm RAF file")]
    NotRaf,
    #[error("RAF header is truncated")]
    Truncated,
    #[error("RAF has no embedded JPEG")]
    NoEmbeddedJpeg,
    #[error("embedded JPEG lies outside the file")]
    OutOfBounds,
    #[error("embedded JPEG is larger than {MAX_EMBEDDED_JPEG_LEN} bytes")]
    TooLarge,
    #[error("embedded data is not a JPEG")]
    NotJpeg,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn be_u32(bytes: &[u8], at: usize) -> Result<u32, RafError> {
    bytes
        .get(at..at + 4)
        .and_then(|slice| slice.try_into().ok())
        .map(u32::from_be_bytes)
        .ok_or(RafError::Truncated)
}

pub fn embedded_jpeg_range(header: &[u8], file_len: u64) -> Result<Range<u64>, RafError> {
    if !header.starts_with(MAGIC) {
        return Err(RafError::NotRaf);
    }
    let offset = u64::from(be_u32(header, JPEG_OFFSET_AT)?);
    let length = u64::from(be_u32(header, JPEG_LENGTH_AT)?);
    if offset == 0 || length == 0 {
        return Err(RafError::NoEmbeddedJpeg);
    }
    if length > MAX_EMBEDDED_JPEG_LEN {
        return Err(RafError::TooLarge);
    }
    let end = offset + length;
    if end > file_len {
        return Err(RafError::OutOfBounds);
    }
    Ok(offset..end)
}

pub fn read_embedded_jpeg(path: &Path) -> Result<Vec<u8>, RafError> {
    read_embedded_jpeg_up_to(path, MAX_EMBEDDED_JPEG_LEN)
}

pub fn read_embedded_jpeg_up_to(path: &Path, limit: u64) -> Result<Vec<u8>, RafError> {
    let mut file = crate::fsx::open_regular(path)?;
    let file_len = file.metadata()?.len();
    let mut header = [0u8; HEADER_LEN];
    file.read_exact(&mut header)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::UnexpectedEof => RafError::Truncated,
            _ => RafError::Io(error),
        })?;
    let range = embedded_jpeg_range(&header, file_len)?;
    file.seek(SeekFrom::Start(range.start))?;
    let length =
        usize::try_from((range.end - range.start).min(limit)).map_err(|_| RafError::TooLarge)?;
    let mut jpeg = vec![0u8; length];
    file.read_exact(&mut jpeg)?;
    if !jpeg.starts_with(&[0xFF, 0xD8]) {
        return Err(RafError::NotJpeg);
    }
    Ok(jpeg)
}

#[cfg(test)]
pub mod fixtures {
    use super::*;

    pub fn raf_with_jpeg(jpeg: &[u8]) -> Vec<u8> {
        let offset = 256u32;
        let mut raf = vec![0u8; offset as usize];
        raf[..MAGIC.len()].copy_from_slice(MAGIC);
        raf[JPEG_OFFSET_AT..JPEG_OFFSET_AT + 4].copy_from_slice(&offset.to_be_bytes());
        let length = u32::try_from(jpeg.len()).unwrap_or(u32::MAX);
        raf[JPEG_LENGTH_AT..JPEG_LENGTH_AT + 4].copy_from_slice(&length.to_be_bytes());
        raf.extend_from_slice(jpeg);
        raf.extend_from_slice(&[0u8; 64]);
        raf
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::raf_with_jpeg;
    use super::*;

    fn header_with(offset: u32, length: u32) -> Vec<u8> {
        let mut header = vec![0u8; HEADER_LEN];
        header[..MAGIC.len()].copy_from_slice(MAGIC);
        header[JPEG_OFFSET_AT..JPEG_OFFSET_AT + 4].copy_from_slice(&offset.to_be_bytes());
        header[JPEG_LENGTH_AT..JPEG_LENGTH_AT + 4].copy_from_slice(&length.to_be_bytes());
        header
    }

    #[test]
    fn finds_embedded_jpeg_range() {
        let range = embedded_jpeg_range(&header_with(1000, 500), 2000);
        assert_eq!(range.ok(), Some(1000..1500));
    }

    #[test]
    fn rejects_wrong_magic() {
        let mut header = header_with(1000, 500);
        header[0] = b'X';
        assert!(matches!(
            embedded_jpeg_range(&header, 2000),
            Err(RafError::NotRaf)
        ));
    }

    #[test]
    fn rejects_truncated_header() {
        let header = header_with(1000, 500);
        assert!(matches!(
            embedded_jpeg_range(&header[..86], 2000),
            Err(RafError::Truncated)
        ));
    }

    #[test]
    fn rejects_range_past_end_of_file() {
        assert!(matches!(
            embedded_jpeg_range(&header_with(1000, 1500), 2000),
            Err(RafError::OutOfBounds)
        ));
    }

    #[test]
    fn rejects_oversized_embedded_jpeg() {
        assert!(matches!(
            embedded_jpeg_range(&header_with(u32::MAX, u32::MAX), u64::MAX),
            Err(RafError::TooLarge)
        ));
    }

    #[test]
    fn rejects_zero_offset_or_length() {
        assert!(matches!(
            embedded_jpeg_range(&header_with(0, 10), 2000),
            Err(RafError::NoEmbeddedJpeg)
        ));
        assert!(matches!(
            embedded_jpeg_range(&header_with(10, 0), 2000),
            Err(RafError::NoEmbeddedJpeg)
        ));
    }

    #[test]
    fn reads_embedded_jpeg_from_file() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("DSCF0001.RAF");
        let jpeg = [0xFF, 0xD8, 1, 2, 3, 0xFF, 0xD9];
        std::fs::write(&path, raf_with_jpeg(&jpeg))?;
        assert_eq!(read_embedded_jpeg(&path)?, jpeg);
        Ok(())
    }

    #[test]
    fn rejects_embedded_data_that_is_not_jpeg() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("DSCF0001.RAF");
        std::fs::write(&path, raf_with_jpeg(&[1, 2, 3, 4]))?;
        assert!(matches!(read_embedded_jpeg(&path), Err(RafError::NotJpeg)));
        Ok(())
    }

    #[test]
    fn rejects_short_file() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("short.RAF");
        std::fs::write(&path, MAGIC)?;
        assert!(matches!(
            read_embedded_jpeg(&path),
            Err(RafError::Truncated)
        ));
        Ok(())
    }
}
