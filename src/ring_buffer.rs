use std::{marker::PhantomData, num::NonZero, sync::atomic::AtomicUsize};

use crossbeam_utils::CachePadded;

use crate::{
    shm::{Shm, ShmError},
    shm_cell::ShmCell,
};

#[derive(Debug)]
pub(crate) struct RingBuffer<T> {
    ctrl: ShmCell<State>,
    data: Shm,
    _marker: PhantomData<T>,
}

#[derive(Debug, Default)]
struct State {
    write_idx: CachePadded<AtomicUsize>,
    read_idx: CachePadded<AtomicUsize>,
}

impl<T> RingBuffer<T> {
    pub fn new(capacity: NonZero<usize>) -> Result<Self, ShmError> {
        let data_size = capacity
            .get()
            .checked_mul(size_of::<T>())
            .and_then(NonZero::new)
            .ok_or(ShmError::InvalidSize)?;

        let data = Shm::new("blob", data_size)?;
        let ctrl = ShmCell::new(State::default())?;

        Ok(Self {
            ctrl,
            data,
            _marker: PhantomData,
        })
    }
}
