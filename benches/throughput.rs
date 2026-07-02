use std::hint::black_box;
use std::num::NonZero;
use std::thread;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use libipc::RingBuffer;

const CAPACITY: usize = 1024;
const ITEMS: u64 = 1 << 16;

/// Two handles onto the same shared ring: producer + consumer.
fn make_pair() -> (RingBuffer<u64>, RingBuffer<u64>) {
    let cap = NonZero::new(CAPACITY).unwrap();
    let a = RingBuffer::<u64>::new(cap).unwrap();

    let data_fd = a.data_fd().try_clone_to_owned().unwrap();
    let state_fd = a.state_fd().try_clone_to_owned().unwrap();
    let b = unsafe { RingBuffer::<u64>::from_fds(data_fd, state_fd, cap) }.unwrap();

    (a, b)
}

fn spsc(c: &mut Criterion) {
    let mut group = c.benchmark_group("ring_buffer");
    group.throughput(Throughput::Elements(ITEMS));

    group.bench_function("spsc_u64", |b| {
        b.iter_batched(
            make_pair,
            |(mut producer, mut consumer)| {
                let sender = thread::spawn(move || {
                    for i in 0..ITEMS {
                        producer.push(i);
                    }
                });
                for _ in 0..ITEMS {
                    black_box(consumer.pop());
                }
                sender.join().unwrap();
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, spsc);
criterion_main!(benches);
