use std::num::NonZero;

use crate::shm::Shm;

mod shm;
mod shm_cell;

fn main() {
    let shm = Shm::new("shm", NonZero::new(0x1000usize).unwrap()).unwrap();
    println!("ptr: {:p}, size: {:#x}", shm.as_ptr(), shm.size().get());
}
