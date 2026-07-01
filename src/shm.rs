use std::num::NonZero;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::ptr::NonNull;

use nix::sys::memfd::{MFdFlags, memfd_create};
use nix::sys::mman::{MapFlags, ProtFlags, mmap, munmap};
use nix::unistd::{Whence, ftruncate, lseek};

#[derive(thiserror::Error, Debug)]
pub(crate) enum ShmError {
    #[error("Unexpected error: {0}")]
    Unexpected(#[from] nix::Error),

    #[error("invalid size")]
    InvalidSize,
}

pub(crate) struct Shm {
    ptr: NonNull<u8>,
    size: NonZero<usize>,
    fd: OwnedFd,
}

impl Shm {
    pub(crate) fn new(name: &str, size: NonZero<usize>) -> Result<Self, ShmError> {
        let fd = memfd_create(name, MFdFlags::MFD_CLOEXEC)?;
        ftruncate(
            &fd,
            size.get().try_into().map_err(|_| ShmError::InvalidSize)?,
        )?;

        let ptr = mmap_fd(&fd, size)?;
        Ok(Shm { ptr, size, fd })
    }

    pub(crate) fn from_fd(fd: OwnedFd) -> Result<Self, ShmError> {
        let size = lseek(&fd, 0, Whence::SeekEnd)?;
        let size = usize::try_from(size)
            .ok()
            .and_then(NonZero::new)
            .ok_or(ShmError::InvalidSize)?;

        let ptr = mmap_fd(&fd, size)?;
        Ok(Shm { ptr, size, fd })
    }

    pub(crate) fn as_ptr(&self) -> NonNull<u8> {
        self.ptr
    }

    pub(crate) fn size(&self) -> NonZero<usize> {
        self.size
    }

    pub(crate) fn fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

fn mmap_fd(fd: &OwnedFd, size: NonZero<usize>) -> Result<NonNull<u8>, ShmError> {
    let ptr = unsafe {
        mmap(
            None,
            size,
            ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
            MapFlags::MAP_SHARED,
            fd,
            0,
        )
    }?
    .cast::<u8>();

    Ok(ptr)
}

impl Drop for Shm {
    fn drop(&mut self) {
        unsafe { munmap(self.ptr.cast(), self.size.get()).unwrap() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_map_again() {
        let a = Shm::new("test", NonZero::new(0x1000usize).unwrap()).unwrap();
        let fd = a.fd.try_clone().unwrap();
        let b = Shm::from_fd(fd).unwrap();

        unsafe { a.as_ptr().as_ptr().write(42) };
        assert_eq!(unsafe { b.as_ptr().as_ptr().read() }, 42);

        unsafe { b.as_ptr().as_ptr().write(99) };
        assert_eq!(unsafe { a.as_ptr().as_ptr().read() }, 99);
    }
}
