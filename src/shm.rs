use std::num::NonZero;
use std::os::fd::OwnedFd;
use std::ptr::NonNull;

use nix::sys::memfd::{MFdFlags, memfd_create};
use nix::sys::mman::{MapFlags, ProtFlags, mmap, munmap};
use nix::unistd::{Whence, ftruncate, lseek};

#[derive(thiserror::Error, Debug)]
pub(crate) enum ShmError {
    #[error("nix error: {0}")]
    Nix(#[from] nix::Error),

    #[error("Not a valid shared memory file descriptor")]
    InvalidFd,
}

pub(crate) struct SharedMemory {
    pub(crate) ptr: NonNull<u8>,
    pub(crate) size: NonZero<usize>,
    _fd: OwnedFd,
}

impl SharedMemory {
    pub(crate) fn new(name: &str, size: NonZero<usize>) -> Result<Self, ShmError> {
        let fd = memfd_create(name, MFdFlags::MFD_CLOEXEC)?;
        ftruncate(&fd, size.get().try_into().unwrap())?;

        let ptr = mmap_fd(&fd, size)?;
        Ok(SharedMemory { ptr, size, _fd: fd })
    }

    pub(crate) fn from_fd(fd: OwnedFd) -> Result<Self, ShmError> {
        let size = lseek(&fd, 0, Whence::SeekEnd)?;
        let size = usize::try_from(size)
            .ok()
            .and_then(NonZero::new)
            .ok_or(ShmError::InvalidFd)?;

        let ptr = mmap_fd(&fd, size)?;
        Ok(SharedMemory { ptr, size, _fd: fd })
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

impl Drop for SharedMemory {
    fn drop(&mut self) {
        unsafe { munmap(self.ptr.cast(), self.size.get()).unwrap() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_map_again() {
        let mut a = SharedMemory::new("test", NonZero::new(0x1000usize).unwrap()).unwrap();
        let fd = a._fd.try_clone().unwrap();
        let mut b = SharedMemory::from_fd(fd).unwrap();

        unsafe { a.ptr.as_ptr().write(42) };
        assert_eq!(unsafe { b.ptr.as_ptr().read() }, 42);

        unsafe { b.ptr.as_ptr().write(99) };
        assert_eq!(unsafe { a.ptr.as_ptr().read() }, 99);
    }
}

