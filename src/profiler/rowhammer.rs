#![allow(dead_code)]
#![allow(unused_variables)]

use std::{arch::x86_64::_mm_clflush, mem::size_of, time::Instant};

use memmap2::MmapMut;
use procfs::{
    process::{PageMap, Process},
    ProcResult,
};
use rand::Rng;

use crate::{
    profiler::utils::{get_page_frame_number, setup_mapping, Consts},
    Bridge,
};

use super::utils::{Page, Row};

const NO_OF_READS: u64 = 27 * 100 * 1000 * 4;
const STRIPE: [u64; 3] = [0x00FF00FF00FF00FF, 0, 0x00FF00FF00FF00FF];

fn rowhammer(above_page: *mut u8, below_page: *mut u8) {
    let above_page64 = above_page as *mut u64;
    let below_page64 = below_page as *mut u64;
    for _ in 0..NO_OF_READS {
        unsafe {
            _mm_clflush(above_page);
            above_page64.read_volatile();
            _mm_clflush(below_page);
            below_page64.read_volatile();
        }
    }
}

fn collect_pages_by_row(mmap: &mut MmapMut, pagemap: &mut PageMap, row_size: usize) -> Vec<Row> {
    let base_ptr = mmap.as_mut_ptr();
    let mut rows = Vec::new();
    for offset in (0..mmap.len()).step_by(Consts::PAGE_SIZE) {
        unsafe {
            let virtual_addr = base_ptr.add(offset);
            if let Ok(pfn) = get_page_frame_number(pagemap, virtual_addr) {
                let physical_addr = pfn as usize * Consts::PAGE_SIZE;
                let presumed_row_index = physical_addr as usize / row_size;
                // If the row index is larger than the number of rows, we
                // push new rows until we have enough.
                if presumed_row_index >= rows.len() {
                    for i in rows.len()..presumed_row_index + 1 {
                        rows.push(Row::new(i));
                    }
                }
                rows[presumed_row_index].push(Page::new(virtual_addr, pfn));
            }
        }
    }
    rows
}

fn init_rows(rows: [&[Page]; 3]) {
    for (row, pattern) in rows.iter().zip(STRIPE) {
        for page in *row {
            let ptr = page.virt_addr as *mut u64;
            for i in 0..Consts::PAGE_SIZE / size_of::<u64>() {
                unsafe {
                    *ptr.add(i) = pattern;
                    _mm_clflush(page.virt_addr);
                }
            }
        }
    }
}

/// Finds the indices (0-7) of the set bits in `byte`.
///
/// # Returns
///
/// A `Vec` containing the indices of the set bits in `byte`.
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
/// A `Vec` with tuples containing a `Page` of the page with the flipped
/// bit, and a `Vec` containing all indices of the flipped bits.
fn find_flips(row: &[Page]) -> (Vec<(Page, Vec<usize>)>, u64) {
    let mut no_of_flips = 0;
    let mut flips = Vec::new();
    for page in row {
        let mut flipped_indexes = Vec::new();
        for i in 0..Consts::PAGE_SIZE {
            unsafe {
                _mm_clflush(page.virt_addr);
                let byte = *(page.virt_addr).add(i);
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

fn hammer_all_reachable_pages(
    mmap: &mut MmapMut,
    cores: u8,
    dimms: u8,
    bridge: Bridge,
) -> ProcResult<()> {
    let pagemap = &mut Process::myself()?.pagemap()?;
    let row_size = 128 * 1024 * dimms as usize;

    println!("Collecting all pages in all rows...");
    let pages_by_row = collect_pages_by_row(mmap, pagemap, row_size);

    // let mut rng = rand::thread_rng();
    let mut total_flips = 0;
    let mut no_of_rows_tested: u32 = 0;

    if pages_by_row.len() < 3 {
        eprintln!(
            "[!] Can't hammer rows - only got {} rows total. Are you running as sudo?",
            pages_by_row.len()
        );
        return Ok(());
    }

    let mut rng = rand::thread_rng();
    'main: for above_row_index in 0..pages_by_row.len() - 2 {
        let above_row_index = rng.gen::<usize>() % (pages_by_row.len() - 2);
        let target_row_index = above_row_index + 1;
        let below_row_index = above_row_index + 2;

        let above_row = &pages_by_row[above_row_index];
        let target_row = &pages_by_row[target_row_index];
        let below_row = &pages_by_row[below_row_index];

        for i in 0..3 {
            if pages_by_row[above_row_index + i].len() != row_size as usize / Consts::PAGE_SIZE {
                eprintln!(
                    "[!] Can't hammer row {target_row_index} - only got {} (of {}) pages from row {}.",
                    pages_by_row[above_row_index + i].len(),
                    row_size as usize / Consts::PAGE_SIZE,
                    above_row_index + i,
                );
                continue 'main;
            }
        }

        let rows = [&above_row[..], &target_row[..], &below_row[..]];
        init_rows(rows);

        let before = Instant::now();
        for (above_row_page, below_row_page) in above_row.into_iter().zip(below_row.into_iter()) {
            // Filter out pages which are not in the same column
            let above_row_mapping = above_row_page.dram_mapping(bridge, dimms);
            let below_row_mapping = below_row_page.dram_mapping(bridge, dimms);
            if above_row_mapping != below_row_mapping {
                // eprintln!(
                //     "Mapping mismatch: {:#x} != {:#x}",
                //     above_row_mapping, below_row_mapping
                // );
                continue;
            }

            // RELEASE THE BEAST
            println!("Hammering rows {above_row_index}-{below_row_index}...");
            rowhammer(above_row_page.virt_addr, below_row_page.virt_addr);

            println!(
                "Hammering row {target_row_index} took {:.2?} seconds",
                before.elapsed()
            );

            // Count the flips in the row after hammering it
            let (flips, no_of_flips) = find_flips(&target_row[..]);
            total_flips += no_of_flips;
            if flips.is_empty() {
                println!("No flips found in row {target_row_index}.");
            } else {
                println!("Found {no_of_flips} flips in row {target_row_index}:");
            }

            for (flipped_page, bit_indices) in flips {
                let pfn = get_page_frame_number(pagemap, flipped_page.virt_addr)?;
                println!("\tpfn: {pfn}\tflipped bits at: {:?}", bit_indices);
            }

            no_of_rows_tested += 1;
            println!(
                "So far: {:.4} flips per row ({no_of_rows_tested} rows tested, {total_flips} flips total)\n",
                total_flips as f64 / no_of_rows_tested as f64,
            );
        }
    }
    Ok(())
}

pub(crate) fn main(fraction_of_phys_memory: f64, cores: u8, dimms: u8, bridge: Bridge) {
    println!("Setting up memory map...");
    let mut mmap = setup_mapping(fraction_of_phys_memory);
    hammer_all_reachable_pages(&mut mmap, cores, dimms, bridge).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_rows() -> ProcResult<()> {
        const LENGTH: usize = 0x1;

        let mut mmap = setup_mapping(0.1);
        let row_size = 128 * 1024 * 2;
        let mut pagemap = Process::myself()?.pagemap()?;
        let pages_by_row = collect_pages_by_row(&mut mmap, &mut pagemap, row_size);

        'main: for row in 0..pages_by_row.len() - 2 {
            let target_row = row + 1;
            for i in 0..3 {
                if pages_by_row[row + i].len() != row_size as usize / Consts::PAGE_SIZE {
                    eprintln!(
                        "[!] Can't hammer row {target_row} - only got {} (of {}) pages from row {}.",
                        pages_by_row[row + i].len(),
                        row_size as usize / Consts::PAGE_SIZE,
                        row + i,
                    );
                    continue 'main;
                }
            }
            let rows = [
                &pages_by_row[row + 0][..],
                &pages_by_row[row + 1][..],
                &pages_by_row[row + 2][..],
            ];
            for row in &rows {
                for page in *row {
                    let ptr = page.virt_addr as *mut u64;
                    for i in 0..row_size / size_of::<u64>() {
                        unsafe {
                            assert_eq!(*ptr.add(i), 0);
                        }
                    }
                }
            }
            init_rows(rows);
            for (row, pattern) in rows.iter().zip(STRIPE) {
                for page in *row {
                    let ptr = page.virt_addr as *mut u64;
                    for i in 0..row_size / size_of::<u64>() {
                        unsafe {
                            assert_eq!(*ptr.add(i), pattern);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
