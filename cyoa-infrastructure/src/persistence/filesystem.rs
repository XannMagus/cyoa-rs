//! Linux directory-relative, no-follow filesystem boundary.
use cyoa_application::persistence::MAX_SAVE_BYTES;
use rustix::fs::{AtFlags, CWD, FlockOperation, Mode, OFlags, flock, openat, statat};
use std::{
    fs::{self, DirBuilder, File},
    io::{self, Read},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, MetadataExt},
    },
    path::{Path, PathBuf},
};

pub(super) struct Directory {
    pub file: File,
    pub path: PathBuf,
}
fn private_dirs(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("data directory has no parent"))?;
    private_dirs(parent)?;
    match DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {
            File::open(path)?.sync_all()?;
            File::open(parent)?.sync_all()
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists && path.is_dir() => Ok(()),
        Err(e) => Err(e),
    }
}
impl Directory {
    pub fn open(app: &Path, create: bool) -> io::Result<Self> {
        if create {
            private_dirs(app)?;
        }
        let app = app.canonicalize()?;
        let root = app.join("saves");
        if create && !root.try_exists()? {
            match DirBuilder::new().mode(0o700).create(&root) {
                Ok(()) => {
                    File::open(&root)?.sync_all()?;
                    File::open(&app)?.sync_all()?;
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        }
        let file = File::from(openat(
            CWD,
            &root,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        let meta = file.metadata()?;
        if meta.uid() != rustix::process::getuid().as_raw() || meta.mode() & 0o022 != 0 {
            return Err(io::Error::other(
                "saves directory must be user-owned and not writable by group/others",
            ));
        }
        // Tempfile persistence uses an absolute path anchored to the owned fd,
        // so a rename of the configured root cannot redirect replacements.
        let path = PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()));
        Ok(Self { file, path })
    }
    pub fn open_file(&self, name: &str, create: bool) -> io::Result<File> {
        let flags = if create {
            OFlags::RDWR | OFlags::CREATE
        } else {
            OFlags::RDONLY
        };
        let file = File::from(openat(
            &self.file,
            name,
            flags | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )?);
        let m = file.metadata()?;
        if !m.is_file() || m.nlink() != 1 || m.uid() != rustix::process::getuid().as_raw() {
            return Err(io::Error::other(
                "managed entry must be a user-owned regular file with one link",
            ));
        }
        Ok(file)
    }
    pub fn exists(&self, name: &str) -> io::Result<bool> {
        match statat(&self.file, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => Ok(true),
            Err(rustix::io::Errno::NOENT) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
    pub fn checked_optional(&self, name: &str) -> io::Result<()> {
        if self.exists(name)? {
            self.open_file(name, false)?;
        }
        Ok(())
    }
    pub fn lock(&self, name: &str) -> io::Result<File> {
        let file = self.open_file(name, true)?;
        if file.metadata()?.len() != 0 {
            return Err(io::Error::other("slot lock must be empty"));
        }
        flock(&file, FlockOperation::NonBlockingLockExclusive)?;
        Ok(file)
    }
    pub fn read(&self, name: &str) -> io::Result<Vec<u8>> {
        let file = self.open_file(name, false)?;
        if file.metadata()?.len() > MAX_SAVE_BYTES as u64 {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "save exceeds 64 MiB",
            ));
        }
        let mut bytes = vec![];
        file.take(MAX_SAVE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_SAVE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "save exceeds 64 MiB",
            ));
        }
        Ok(bytes)
    }
    pub fn sync(&self) -> io::Result<()> {
        self.file.sync_all()
    }
    pub fn names(&self) -> io::Result<fs::ReadDir> {
        fs::read_dir(&self.path)
    }
    /// Unlinks one directory entry relative to the owned fd; never follows a
    /// symlink and never removes a directory.
    pub fn remove(&self, name: &str) -> io::Result<()> {
        Ok(rustix::fs::unlinkat(&self.file, name, AtFlags::empty())?)
    }
}
