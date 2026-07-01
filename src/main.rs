use std::num::NonZero;

use crate::shm::SharedMemory;

mod shm;

fn main() {
    let shm = SharedMemory::new("shm", NonZero::new(0x1000usize).unwrap()).unwrap();
    println!("ptr: {:p}, size: {:#x}", shm.ptr, shm.size);
}

