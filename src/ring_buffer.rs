use std::{
    mem::MaybeUninit,
    num::NonZero,
    ops::Range,
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
        let mut used = self.writer.wrapping_sub(self.cached_reader);
        let mut free = self.capacity.get() - used;

        if free < data.len() {
            self.cached_reader = unsafe { self.shared.as_ref() }
                .reader
                .load(Ordering::Acquire);
            used = self.writer.wrapping_sub(self.cached_reader);
            free = self.capacity.get() - used;
        }

        if free < data.len() {
            false
        } else {
            // TODO: write data

            self.writer += data.len();
            unsafe { self.shared.as_ref() }
                .writer
                .store(self.writer, Ordering::Release);

            true
        }
    }
}

impl<T> Consumer<T> {
    fn read(&mut self, data: &mut [T]) -> usize {
        let mut used = self.cached_writer.wrapping_sub(self.reader);

        if used < data.len() {
            self.cached_writer = unsafe { self.shared.as_ref() }
                .writer
                .load(Ordering::Acquire);

            used = self.cached_writer.wrapping_sub(self.reader);
        }

        if used < data.len() {
            0
        } else {
            // TODO: read data

            self.reader += data.len();
            unsafe { self.shared.as_ref() }
                .reader
                .store(self.reader, Ordering::Release);

            data.len() // TODO: return amount of elements
        }
    }
}

#[inline]
fn ring_segments(
    head: usize,
    tail: usize,
    capacity: usize,
    mask: usize,
) -> (Range<usize>, Range<usize>) {
    let used = head.wrapping_sub(tail);
    let free = capacity - used;

    let idx = head & mask;
    let until_wrap = capacity - idx;

    let first_len = free.min(until_wrap);
    let second_len = free - first_len;

    (idx..first_len, 0..second_len)
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

        for _ in 0..64 {
            let mut buffer = [0; 16];
            assert_eq!(consumer.read(&mut buffer), 16);
        }

        for _ in 0..64 {
            let buffer = [0; 16];
            assert!(producer.write(&buffer));
        }
    }
}
