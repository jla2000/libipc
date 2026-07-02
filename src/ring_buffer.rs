use std::{
    marker::PhantomData,
    num::NonZero,
    os::fd::{BorrowedFd, OwnedFd},
    sync::atomic::{AtomicU32, AtomicUsize, Ordering},
};

use crossbeam_utils::CachePadded;
use nix::libc::{FUTEX_WAIT, FUTEX_WAKE, SYS_futex, syscall};

use crate::{
    shm::{Shm, ShmError},
    shm_cell::ShmCell,
};

#[derive(Debug)]
pub struct RingBuffer<T> {
    shared_state: ShmCell<State>,
    data: Shm,
    capacity: NonZero<usize>,
    // Per-handle cache of the peer's index, to avoid a cross-core load on the
    // hot path. Refreshed only when the ring looks full (push) or empty (pop).
    cached_reader: usize,
    cached_writer: usize,
    _marker: PhantomData<T>,
}

// SAFETY: RingBuffer coordinates access through atomics in shared memory; the
// underlying pointer is valid to send to another thread/process.
unsafe impl<T: Send> Send for RingBuffer<T> {}

#[derive(Debug, Default)]
struct State {
    writer: CachePadded<AtomicUsize>,
    reader: CachePadded<AtomicUsize>,
    is_empty: CachePadded<AtomicU32>,
    is_full: CachePadded<AtomicU32>,
}

const MAX_SPINS: usize = 100;

impl<T> RingBuffer<T> {
    pub fn new(capacity: NonZero<usize>) -> Result<Self, ShmError> {
        const { assert!(!std::mem::needs_drop::<T>()) };

        // Power-of-two capacity lets the index wrap with a mask instead of `%`.
        if !capacity.get().is_power_of_two() {
            return Err(ShmError::InvalidSize);
        }

        let data_size = capacity
            .get()
            .checked_mul(size_of::<T>())
            .and_then(NonZero::new)
            .ok_or(ShmError::InvalidSize)?;

        let data = Shm::new("blob", data_size)?;
        let ctrl = ShmCell::new(State::default())?;

        Ok(Self {
            shared_state: ctrl,
            data,
            capacity,
            cached_reader: 0,
            cached_writer: 0,
            _marker: PhantomData,
        })
    }

    /// # Safety
    /// `data_fd` and `state_fd` must come from another `RingBuffer<T>` created
    /// with the same `capacity` and element type `T`.
    pub unsafe fn from_fds(
        data_fd: OwnedFd,
        state_fd: OwnedFd,
        capacity: NonZero<usize>,
    ) -> Result<Self, ShmError> {
        const { assert!(!std::mem::needs_drop::<T>()) };

        if !capacity.get().is_power_of_two() {
            return Err(ShmError::InvalidSize);
        }

        let data = Shm::from_fd(data_fd)?;
        let shared_state = unsafe { ShmCell::from_fd(state_fd)? };

        Ok(Self {
            shared_state,
            data,
            capacity,
            cached_reader: 0,
            cached_writer: 0,
            _marker: PhantomData,
        })
    }

    pub fn data_fd(&self) -> BorrowedFd<'_> {
        self.data.fd()
    }

    pub fn state_fd(&self) -> BorrowedFd<'_> {
        self.shared_state.fd()
    }

    pub fn push(&mut self, value: T) {
        let mask = self.capacity.get() - 1;
        // We are the sole writer, so `writer` never changes under us.
        let writer = self.shared_state.as_ref().writer.load(Ordering::Relaxed);
        let mut spins = 0;

        loop {
            if writer - self.cached_reader < self.capacity.get() {
                let base_ptr = self.data.as_ptr().cast::<T>();
                let elem_ptr = unsafe { base_ptr.add(writer & mask) };

                unsafe { elem_ptr.write(value) };

                self.shared_state
                    .as_ref()
                    .writer
                    .store(writer + 1, Ordering::Release);

                // Wake a blocked reader only if one flagged itself empty.
                let is_empty = &self.shared_state.as_ref().is_empty;
                if is_empty.load(Ordering::Relaxed) == 1 {
                    is_empty.store(0, Ordering::Release);
                    futex_wake(is_empty);
                }

                break;
            }

            // Looks full per cached index — pay the cross-core load to confirm.
            self.cached_reader = self.shared_state.as_ref().reader.load(Ordering::Acquire);

            if spins < MAX_SPINS {
                spins += 1;
                std::hint::spin_loop();
                continue;
            }

            let is_full = &self.shared_state.as_ref().is_full;
            is_full.store(1, Ordering::Release);
            futex_wait(is_full, 1);
        }
    }

    pub fn pop(&mut self) -> T {
        let mask = self.capacity.get() - 1;
        // We are the sole reader, so `reader` never changes under us.
        let reader = self.shared_state.as_ref().reader.load(Ordering::Relaxed);
        let mut spins = 0;

        loop {
            if reader != self.cached_writer {
                let base_ptr = self.data.as_ptr().cast::<T>();
                let elem_ptr = unsafe { base_ptr.add(reader & mask) };

                let value = unsafe { elem_ptr.read() };

                self.shared_state
                    .as_ref()
                    .reader
                    .store(reader + 1, Ordering::Release);

                // Wake a blocked writer only if one flagged itself full.
                let is_full = &self.shared_state.as_ref().is_full;
                if is_full.load(Ordering::Relaxed) == 1 {
                    is_full.store(0, Ordering::Release);
                    futex_wake(is_full);
                }

                break value;
            }

            // Looks empty per cached index — pay the cross-core load to confirm.
            self.cached_writer = self.shared_state.as_ref().writer.load(Ordering::Acquire);

            if spins < MAX_SPINS {
                spins += 1;
                std::hint::spin_loop();
                continue;
            }

            let is_empty = &self.shared_state.as_ref().is_empty;
            is_empty.store(1, Ordering::Release);
            futex_wait(is_empty, 1);
        }
    }
}

/// Sleep while `*address == expected`. Returns immediately (EAGAIN) if the
/// value already changed, so the caller must re-check its condition in a loop.
fn futex_wait(address: &AtomicU32, expected: u32) {
    unsafe { syscall(SYS_futex, address.as_ptr(), FUTEX_WAIT, expected, 0) };
}

/// Wake at most one waiter. Returns 0 when none are parked — not an error.
fn futex_wake(address: &AtomicU32) {
    unsafe { syscall(SYS_futex, address.as_ptr(), FUTEX_WAKE, 1) };
}
