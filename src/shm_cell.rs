use std::marker::PhantomData;
use std::num::NonZero;
use std::os::fd::OwnedFd;

use crate::shm::Shm;
use crate::shm::ShmError;

#[derive(Debug)]
pub(crate) struct ShmCell<T> {
    memory: Shm,
    _marker: PhantomData<T>,
}

impl<T> ShmCell<T> {
    const SIZE: NonZero<usize> = const { NonZero::new(size_of::<T>()).unwrap() };

    pub(crate) fn new(value: T) -> Result<Self, ShmError> {
        const { assert!(!std::mem::needs_drop::<T>()) };

        let shm = Shm::new(std::any::type_name::<T>(), Self::SIZE)?;
        let ptr = shm.as_ptr().cast::<T>();

        // SAFETY: Writing to uninitialized memory
        unsafe { ptr.write(value) };

        Ok(Self {
            memory: shm,
            _marker: PhantomData,
        })
    }

    /// # Safety
    /// The given `fd` must be obtained from another `ShmCell<T>` with the same type.
    pub(crate) unsafe fn from_fd(fd: OwnedFd) -> Result<Self, ShmError> {
        const { assert!(!std::mem::needs_drop::<T>()) };

        let shm = Shm::from_fd(fd)?;

        if shm.size() == Self::SIZE {
            Ok(Self {
                memory: shm,
                _marker: PhantomData,
            })
        } else {
            Err(ShmError::InvalidSize)
        }
    }
}

impl<T> AsRef<T> for ShmCell<T> {
    fn as_ref(&self) -> &T {
        // SAFETY: self.memory contains a valid T
        unsafe { self.memory.as_ptr().cast().as_ref() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_and_read_value() {
        let cell_a = ShmCell::new(42u32).unwrap();
        let cell_b: ShmCell<u32> =
            unsafe { ShmCell::from_fd(cell_a.memory.fd().try_clone_to_owned().unwrap()) }.unwrap();

        assert_eq!(*cell_a.as_ref(), 42);
        assert_eq!(*cell_b.as_ref(), 42);
    }

    #[test]
    fn interior_mutability_shared() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let cell_a = ShmCell::new(AtomicU32::new(0)).unwrap();
        let cell_b: ShmCell<AtomicU32> =
            unsafe { ShmCell::from_fd(cell_a.memory.fd().try_clone_to_owned().unwrap()) }.unwrap();

        cell_a.as_ref().store(123, Ordering::Relaxed);
        assert_eq!(cell_b.as_ref().load(Ordering::Relaxed), 123);

        cell_b.as_ref().store(456, Ordering::Relaxed);
        assert_eq!(cell_a.as_ref().load(Ordering::Relaxed), 456);
    }
}
