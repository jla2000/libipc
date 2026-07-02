use std::{
    mem::MaybeUninit,
    num::NonZero,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

use crossbeam_utils::CachePadded;

#[derive(Debug)]
struct Producer<T> {
    writer: usize,
    cached_reader: usize,
    buffer: NonNull<MaybeUninit<T>>,
    capacity: NonZero<usize>,
    mask: usize,
    shared: NonNull<SharedState>,
}

#[derive(Debug)]
struct Consumer<T> {
    reader: usize,
    cached_writer: usize,
    buffer: NonNull<MaybeUninit<T>>,
    capacity: NonZero<usize>,
    mask: usize,
    shared: NonNull<SharedState>,
}

#[derive(Debug, Default)]
#[repr(C)]
struct SharedState {
    writer: CachePadded<AtomicUsize>,
    reader: CachePadded<AtomicUsize>,
}

impl<T> Producer<T> {
    fn write(&mut self, data: &[T]) -> bool {
        let used = self.writer.wrapping_sub(self.cached_reader);
        let free = self.capacity.get() - used;

        if free >= data.len() {
            // TODO: write data

            self.writer += data.len();
            unsafe { self.shared.as_ref() }
                .writer
                .store(self.writer, Ordering::Release);

            true
        } else {
            // Not enough capacity, load the reader
            self.cached_reader = unsafe { self.shared.as_ref() }
                .reader
                .load(Ordering::Acquire);

            false
        }
    }
}

impl<T> Consumer<T> {
    fn read(&mut self, data: &mut [T]) -> bool {
        let used = self.cached_writer.wrapping_sub(self.reader);

        if used >= data.len() {
            // TODO: read data

            self.reader += data.len();
            unsafe { self.shared.as_ref() }
                .reader
                .store(self.reader, Ordering::Release);

            true
        } else {
            // Not enough capacity, load the reader
            self.cached_writer = unsafe { self.shared.as_ref() }
                .writer
                .load(Ordering::Acquire);

            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{mem::MaybeUninit, ptr::NonNull};

    use crate::ring_buffer::{Consumer, Producer, SharedState};

    #[test]
    fn test_write_and_read() {
        let mut shared_state = SharedState::default();
        let mut buffer_memory = [MaybeUninit::<usize>::uninit(); 1024];

        let mut producer = Producer {
            writer: 0,
            cached_reader: 0,
            buffer: NonNull::new(buffer_memory.as_mut_ptr()).unwrap(),
            capacity: buffer_memory.len().try_into().unwrap(),
            mask: buffer_memory.len() - 1,
            shared: NonNull::new(&raw mut shared_state).unwrap(),
        };
        let mut consumer = Consumer {
            reader: 0,
            cached_writer: 0,
            buffer: NonNull::new(buffer_memory.as_mut_ptr()).unwrap(),
            capacity: buffer_memory.len().try_into().unwrap(),
            mask: buffer_memory.len() - 1,
            shared: NonNull::new(&raw mut shared_state).unwrap(),
        };

        for _ in 0..64 {
            let buffer = [0; 16];
            assert!(producer.write(&buffer));
        }

        // first consume fails, need to reload atomics
        let mut buffer = [0; 16];
        assert!(!consumer.read(&mut buffer));

        for _ in 0..64 {
            let mut buffer = [0; 16];
            assert!(consumer.read(&mut buffer));
        }
    }
}
