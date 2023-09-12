#![allow(dead_code)]
#![allow(unused_variables)]

use std::{
    arch::x86_64::_mm_clflush,
    mem::size_of_val,
    time::{Duration, Instant},
};

use memmap2::MmapMut;
use procfs::{
    process::{PageMap, Process},
    ProcResult,
};
use rand::Rng;

use crate::{
    profiler::utils::{get_page_frame_number, setup_mapping, Consts, PageData},
    Bridge,
};

use super::utils::{Page, Row};

const NO_OF_READS: u64 = 27 * 100 * 1000 * 4;
const OFF_ON: u64 = 0x5555555555555555;
const ON_OFF: u64 = 0xaaaaaaaaaaaaaaaa;
const STRIPE: u64 = 0x00FF00FF00FF00FF;
const FRODO_HAMMER: u64 = 0x0100010001000100;
const BLAST: u64 = u64::MAX;

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

fn init_row(row: &[Page], pattern: u64) {
    for page in row {
        let base_ptr = page.virt_addr as *mut u64;
        for i in 0..Consts::PAGE_SIZE / size_of_val(&pattern) {
            unsafe {
                *base_ptr.add(i) = pattern;
                // let ptr = base_ptr.add(i);
                // *ptr = pattern;
                // _mm_clflush(ptr);
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
fn find_flips(page: &Page) -> Vec<usize> {
    let mut flips = Vec::new();
    for i in 0..Consts::PAGE_SIZE {
        unsafe {
            _mm_clflush(page.virt_addr);
            let byte = *(page.virt_addr).add(i);
            if byte != 0 {
                for bit_index in index_of_set_bits(byte) {
                    flips.push(i * 8 + bit_index);
                }
            }
        }
    }
    flips
}

fn hammer_all_reachable_pages(
    mmap: &mut MmapMut,
    cores: u8,
    dimms: u8,
    bridge: Bridge,
) -> ProcResult<()> {
    let pagemap = &mut Process::myself()?.pagemap()?;
    let row_size = 128 * 1024 * dimms as usize;
    let pattern = FRODO_HAMMER;

    println!("Collecting all pages in all rows...");
    let pages_by_row = collect_pages_by_row(mmap, pagemap, row_size);

    // let mut rng = rand::thread_rng();
    let mut total_flips = 0;
    let mut rows_tested: u32 = 0;

    if pages_by_row.len() < 3 {
        eprintln!(
            "[!] Can't hammer rows - only got {} rows total. Are you running as sudo?",
            pages_by_row.len()
        );
        return Ok(());
    }

    let mut row_data = Vec::new();

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

        // Initialize rows (above and below get aggressor pattern, i.e. 0b0101, and target row gets zeroed)
        init_row(&above_row[..], pattern);
        init_row(&target_row[..], 0);
        init_row(&below_row[..], pattern);

        let mut dram_map_time = Duration::default();
        let mut hammering_time = Duration::default();
        for above_page in above_row {
            let before = Instant::now();
            for below_page in below_row {
                // Filter out pages which are not in the same bank
                let above_row_bank = above_page.bank_index(bridge, dimms);
                let below_row_bank = below_page.bank_index(bridge, dimms);
                if above_row_bank != below_row_bank {
                    // eprintln!(
                    //     "Bank mismatch: {} != {}",
                    //     above_row_bank, below_row_bank
                    // );
                    continue;
                }
                dram_map_time += before.elapsed();

                // RELEASE THE BEAST
                let before = Instant::now();
                rowhammer(above_page.virt_addr, below_page.virt_addr);
                hammering_time += before.elapsed();
                break;
            }
        }
        rows_tested += 1;

        println!(
            "Hammering row {} took {:.2?} seconds ({:.2?} spent hashing, {:.2?} spent hammering)",
            target_row_index,
            dram_map_time + hammering_time,
            dram_map_time,
            hammering_time,
        );

        // Collect data into `row_data` for each row
        let mut target_row = target_row.clone();
        for ((above_page, target_page), below_page) in above_row
            .into_iter()
            .zip(&mut target_row)
            .zip(below_row.into_iter())
        {
            let flips = find_flips(target_page);
            total_flips += flips.len();
            if flips.len() > 0 {
                println!(
                    "{} flips found in page {:#x} at indices {:?}",
                    flips.len(),
                    target_page.pfn,
                    flips,
                );
            }
            target_page.data = Some(PageData::new(above_page, below_page, flips));
        }
        row_data.push(target_row);

        let pages_tested = rows_tested * (row_size / Consts::PAGE_SIZE) as u32;
        println!(
            "So far: {:.2} flips per page ({} flips total over {} pages tested)\n",
            total_flips as f64 / pages_tested as f64,
            total_flips,
            pages_tested,
        );
    }
    println!("{:?}", row_data);
    Ok(())
}

pub(crate) fn main(fraction_of_phys_memory: f64, cores: u8, dimms: u8, bridge: Bridge) {
    println!("Setting up memory map...");
    let mut mmap = setup_mapping(fraction_of_phys_memory);
    hammer_all_reachable_pages(&mut mmap, cores, dimms, bridge).unwrap();
}
