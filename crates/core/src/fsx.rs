use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::Path;

#[cfg(unix)]
fn options() -> OpenOptions {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    options
}

#[cfg(windows)]
fn options() -> OpenOptions {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    options
}

#[cfg(not(any(unix, windows)))]
fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true);
    options
}

pub fn open_regular(path: &Path) -> io::Result<File> {
    let file = options().open(path)?;
    if !file.metadata()?.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a regular file", path.display()),
        ));
    }
    Ok(file)
}

pub fn read_limited(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    open_regular(path)?.take(limit).read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    match std::fs::hard_link(from, to) {
        Ok(()) => std::fs::remove_file(from).inspect_err(|_| {
            let _ = std::fs::remove_file(to);
        }),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(error),
        Err(_) => match std::fs::symlink_metadata(to) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => std::fs::rename(from, to),
            _ => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists", to.display()),
            )),
        },
    }
}

pub fn is_local_path(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        match path.components().next() {
            Some(Component::Prefix(prefix)) => {
                matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
            }
            _ => false,
        }
    }
    #[cfg(not(windows))]
    {
        path.is_absolute()
    }
}

pub fn create_private_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_regular_files() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("a.jpg");
        std::fs::write(&path, b"abcdef")?;
        assert_eq!(read_limited(&path, 3)?, b"abc");
        Ok(())
    }

    #[test]
    fn renames_without_replacing() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let from = dir.path().join("a.jpg");
        let to = dir.path().join("b.jpg");
        std::fs::write(&from, b"new")?;
        std::fs::write(&to, b"old")?;
        let error = rename_noreplace(&from, &to).err().map(|error| error.kind());
        assert_eq!(error, Some(io::ErrorKind::AlreadyExists));
        assert_eq!(std::fs::read(&to)?, b"old");
        assert_eq!(std::fs::read(&from)?, b"new");
        let free = dir.path().join("c.jpg");
        rename_noreplace(&from, &free)?;
        assert!(!from.exists());
        assert_eq!(std::fs::read(&free)?, b"new");
        Ok(())
    }

    #[test]
    fn only_absolute_local_paths_are_local() {
        assert!(!is_local_path(Path::new("relative/folder")));
        assert!(!is_local_path(Path::new(r"\\attacker.example\share")));
        #[cfg(windows)]
        {
            assert!(is_local_path(Path::new(r"C:\Photos")));
            assert!(!is_local_path(Path::new(r"\\?\UNC\host\share")));
            assert!(!is_local_path(Path::new(r"\\.\pipe\x")));
        }
        #[cfg(not(windows))]
        assert!(is_local_path(Path::new("/photos")));
    }

    #[test]
    fn refuses_directories() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        assert!(open_regular(dir.path()).is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinks_and_fifos() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let target = dir.path().join("target.jpg");
        std::fs::write(&target, b"x")?;
        let link = dir.path().join("link.jpg");
        std::os::unix::fs::symlink(&target, &link)?;
        assert!(open_regular(&link).is_err());
        let fifo = dir.path().join("fifo.jpg");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .is_ok_and(|status| status.success());
        if made {
            assert!(open_regular(&fifo).is_err());
        }
        Ok(())
    }
}
