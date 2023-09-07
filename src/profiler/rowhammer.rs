#![allow(dead_code)]
#![allow(unused_variables)]

use std::arch::x86_64::_mm_clflush;

use memmap2::MmapMut;
use procfs::{
    process::{PageMap, Process},
    ProcResult,
};

use crate::profiler::utils::{setup_mapping, get_page_frame_number, Consts};

const NO_OF_READS: u64 = 27 * 100 * 1000 * 4;
const STRIPE: [u8; 3] = [0x00FF, 0, 0x00FF];

fn rowhammer(above: *mut u8, below: *mut u8) -> u64 {
    // let t0 = rdtsc();
    let mut sum = 0;
    for _ in 0..NO_OF_READS {
        for ptr in [above, below] {
            unsafe {
                // To avoid the compiler optimizing out the loop (it might or might not do this)
                sum += ptr.read_volatile() as u64;
                _mm_clflush(ptr);
            }
        }
    }
    sum
}

fn collect_pages_by_row(
    mmap: &mut MmapMut,
    pagemap: &mut PageMap,
    row_size: u64,
) -> ProcResult<Vec<Vec<*mut u8>>> {
    let base_ptr = mmap.as_mut_ptr();
    let mut pages_by_row: Vec<Vec<*mut u8>> = vec![Vec::new(); mmap.len() / Consts::PAGE_SIZE];
    for i in 0..pages_by_row.len() {
        let offset = i * Consts::PAGE_SIZE;
        unsafe {
            let virtual_addr = base_ptr.add(offset);
            if let Ok(pfn) = get_page_frame_number(pagemap, virtual_addr as usize) {
                let physical_addr = pfn * Consts::PAGE_SIZE as u64;
                let presumed_row_index = physical_addr / row_size;
                pages_by_row[presumed_row_index as usize].push(virtual_addr);
            }
        }
    }
    Ok(pages_by_row)
}

fn init_rows(rows: [&[*mut u8]; 3], patterns: [u8; 3]) {
    for (row, pattern) in rows.iter().zip(patterns) {
        for page in *row {
            for i in 0..Consts::PAGE_SIZE {
                unsafe {
                    *(*page).add(i) = pattern;
                    _mm_clflush(*page);
                }
            }
        }
    }
}

fn index_of_set_bits(byte: u8) -> Vec<usize> {
    let mut indices = Vec::new();
    for i in 0..8 {
        if byte & (0b1 << i) > 0 {
            indices.push(i);
        }
    }
    indices
}

/// Finds flipped (non-zero) bits in `row`.
///
/// # Returns
///
/// A `Vec` with tuples containing a raw pointer to the page with the flipped
/// bit, and a `Vec` containing all indices of the flipped bits.
fn find_flips(row: &[*mut u8]) -> (Vec<(*mut u8, Vec<usize>)>, u64) {
    let mut no_of_flips = 0;
    let mut flips = Vec::new();
    for page in row {
        let mut flipped_indexes = Vec::new();
        for i in 0..Consts::PAGE_SIZE {
            unsafe {
                _mm_clflush(*page);
                let byte = *(*page).add(i);
                if byte != 0 {
                    for bit_index in index_of_set_bits(byte) {
                        flipped_indexes.push(i * 8 + bit_index);
                        no_of_flips += 1;
                    }
                }
            }
        }
        if !flipped_indexes.is_empty() {
            flips.push((*page, flipped_indexes));
        }
    }
    (flips, no_of_flips)
}

fn hammer_all_reachable_pages(mmap: &mut MmapMut, cores: u8, dimms: u8) -> ProcResult<()> {
    let proc = Process::myself()?;
    let mut pagemap = proc.pagemap()?;
    let row_size = 128 * 1024 * dimms as u64;

    println!("Collecting all pages in all rows...");
    let pages_by_row = collect_pages_by_row(mmap, &mut pagemap, row_size)?;

    println!("Starting profiling...");

    // let mut rng = rand::thread_rng();
    let mut total_flips = 0;
    'main: for row in 0..pages_by_row.len() - 2 {
        // let row = rng.gen::<usize>() % pages_by_row.len();
        for i in 0..3 {
            if pages_by_row[row + i].len() != row_size as usize / Consts::PAGE_SIZE {
                eprintln!(
                    "[!] Can't hammer row {} - only got {} (of {}) pages from row {}.",
                    row,
                    pages_by_row[row + i].len(),
                    row_size as usize / Consts::PAGE_SIZE,
                    row + i,
                );
                continue 'main;
            }
        }

        println!(
            "Hammering row {} (total rows: {}).",
            row + 1,
            pages_by_row[row].len(),
        );

        let mut no_of_rows_tested = 0;
        for above_row_page in &pages_by_row[row] {
            for below_row_page in &pages_by_row[row + 2] {
                no_of_rows_tested += 1;
                let rows = [
                    &pages_by_row[row + 0][..],
                    &pages_by_row[row + 1][..],
                    &pages_by_row[row + 2][..],
                ];
                // Set middle row to zeroes and adjacent rows to 0x00FF, repeating
                init_rows(rows, STRIPE);

                // RELEASE THE BEAST
                rowhammer(*above_row_page, *below_row_page);

                // Now count the flips
                let (flips, no_of_flips) = find_flips(rows[1]);
                total_flips += no_of_flips;
                if flips.is_empty() {
                    println!("No flips found.");
                } else {
                    println!("Found {} flips in row {}:", no_of_flips, row + 1);
                }
                for (flipped_page, bit_indices) in flips {
                    let pfn = get_page_frame_number(&mut pagemap, flipped_page as usize)?;
                    println!("\tpfn: {}\tflipped bits at: {:?}", pfn, bit_indices);
                }

                println!(
                    "\nSo far: {:.4} flips per row. {} flips total.",
                    total_flips as f64 / no_of_rows_tested as f64,
                    total_flips,
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn main(fraction_of_phys_memory: f64, cores: u8, dimms: u8) {
    println!("Setting up memory map...");
    let mut mmap = setup_mapping(fraction_of_phys_memory);
    hammer_all_reachable_pages(&mut mmap, cores, dimms).unwrap();
}
