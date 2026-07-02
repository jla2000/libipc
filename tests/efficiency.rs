//! Efficiency: when the ring is starved, the blocked side must sleep on the
//! futex, not busy-spin. Measured as process CPU time / wall time during a
//! slow-producer run. Futex sleep -> ratio near 0. A spinlock -> ratio ~1.0.

use std::num::NonZero;
use std::thread;
use std::time::{Duration, Instant};

use libipc::RingBuffer;
use nix::libc::{CLOCK_PROCESS_CPUTIME_ID, clock_gettime, timespec};

const CAPACITY: usize = 16;
const ITEMS: u64 = 30;
const GAP: Duration = Duration::from_millis(10); // producer stalls -> consumer starves

fn make_pair() -> (RingBuffer<u64>, RingBuffer<u64>) {
    let cap = NonZero::new(CAPACITY).unwrap();
    let a = RingBuffer::<u64>::new(cap).unwrap();
    let data_fd = a.data_fd().try_clone_to_owned().unwrap();
    let state_fd = a.state_fd().try_clone_to_owned().unwrap();
    let b = unsafe { RingBuffer::<u64>::from_fds(data_fd, state_fd, cap) }.unwrap();
    (a, b)
}

fn cpu_time() -> Duration {
    let mut ts = timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: writing a valid timespec through a valid pointer.
    unsafe { clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &mut ts) };
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

#[test]
fn futex_sleeps_when_starved() {
    let (mut producer, mut consumer) = make_pair();

    let wall_start = Instant::now();
    let cpu_start = cpu_time();

    let sender = thread::spawn(move || {
        for i in 0..ITEMS {
            thread::sleep(GAP);
            producer.push(i);
        }
    });
    for _ in 0..ITEMS {
        consumer.pop();
    }
    sender.join().unwrap();

    let wall = wall_start.elapsed();
    let cpu = cpu_time() - cpu_start;
    let ratio = cpu.as_secs_f64() / wall.as_secs_f64();

    eprintln!("wall={wall:?} cpu={cpu:?} cpu/wall={ratio:.4}");

    // Sleeping correctly keeps CPU far below wall. A busy-spin would sit near
    // 1.0 (or higher with two spinning threads). 0.5 is a wide safety margin.
    assert!(
        ratio < 0.5,
        "ring busy-spun while starved: cpu/wall={ratio:.4}"
    );
}
