#![allow(dead_code)]
#![allow(unused_variables)]

use std::arch::asm;

use clap::Parser;
use memmap2::{MmapMut, MmapOptions};

use super::utils::{get_phys_memory_size, Consts};

const FRACTION_OF_PHYSICAL_MEMORY: f64 = 0.6;
const NO_OF_READS: u64 = 27 * 100 * 1000 * 4;

/// Setup the memory mapping used for the memory profiling
pub(crate) fn setup_mapping() -> MmapMut {
    let mut mmap = MmapOptions::new()
        .len((get_phys_memory_size() as f64 * FRACTION_OF_PHYSICAL_MEMORY) as usize)
        .map_anon()
        .expect("failed to setup memory mapping");

    let ptr = mmap.as_mut_ptr() as *mut u64;

    // Initialize the memory mapping so pages are not empty
    for i in (0..mmap.len()).step_by(Consts::PAGE_SIZE) {
        unsafe {
            *ptr.add(i) = i as u64;
        }
    }
    mmap
}

// fn rdtsc() -> u64 {
//     let mut a: u32;
//     let mut d: u32;
//     unsafe {
//         asm!("cpuid rax, rbx, rcx, rdx", "rdtscp {a:e}, {d:e}, rcx", a = out(reg) a, d = out(reg) d);
//     }
//     ((d as u64) << 32) | (a as u64)
// }

struct Range {
    start: *mut u64,
    end: *mut u64,
}

fn rowhammer(ranges: [Range; 2], no_of_reads: u64) -> u64 {
    // let t0 = rdtsc();
    let mut sum = 0;
    for _ in 0..no_of_reads {
        for range in &ranges {
            let ptr = range.start;
            unsafe {
                // To avoid the compiler optimizing out the loop (it might or might not do this)
                sum += ptr.read_volatile() as u64;
                asm!("clflush (%0), r, {ptr:e}", ptr = in(reg) ptr);
            }
        }
    }
    sum
}

fn hammer_all_reachable_pages(mmap: MmapMut, no_of_reads: u64) {
    todo!("Implement the big boi function");
}

fn hammer_all_reachable_rows(fraction_of_phys_memory: f64, cores: u8, dimms: u8, no_of_reads: u64) {
    let mmap = setup_mapping();
}

#[derive(Parser, Debug)]
struct Args {
    fraction_of_phys_memory: f64,
    cores: u8,
    dimms: u8,
}

pub(crate) fn main() {
    let args = Args::parse();
    hammer_all_reachable_rows(
        args.fraction_of_phys_memory,
        args.cores,
        args.dimms,
        NO_OF_READS,
    );
}
