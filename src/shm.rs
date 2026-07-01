use std::num::NonZero;
use std::os::fd::OwnedFd;
use std::ptr::NonNull;

use nix::sys::memfd::{MFdFlags, memfd_create};
use nix::sys::mman::{MapFlags, ProtFlags, mmap, munmap};
use nix::unistd::ftruncate;

pub(crate) struct SharedMemory {
    pub(crate) ptr: NonNull<u8>,
    pub(crate) size: NonZero<usize>,
    _fd: OwnedFd,
}

impl SharedMemory {
    pub(crate) fn new(name: &str, size: NonZero<usize>) -> nix::Result<Self> {
        let fd = memfd_create(name, MFdFlags::MFD_CLOEXEC)?;
        ftruncate(&fd, size.get().try_into().unwrap())?;

        let ptr = mmap_fd(&fd, size)?;
        Ok(SharedMemory { ptr, size, _fd: fd })
    }

    pub(crate) fn map(fd: OwnedFd, size: NonZero<usize>) -> nix::Result<Self> {
        let ptr = mmap_fd(&fd, size)?;
        Ok(SharedMemory { ptr, size, _fd: fd })
    }
}

fn mmap_fd(fd: &OwnedFd, size: NonZero<usize>) -> nix::Result<NonNull<u8>> {
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
