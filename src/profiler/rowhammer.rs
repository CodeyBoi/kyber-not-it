use std::{arch::x86_64::_mm_clflush, mem::size_of_val, time::Instant};

use memmap2::MmapMut;
use procfs::ProcResult;
use rand::Rng;

use crate::{
    profiler::utils::{collect_pages_by_row, setup_mapping, Consts, Page, PageData},
    Bridge,
};

const NO_OF_READS: u64 = 27 * 100 * 1000 * 4 / 4;

// const OFF_ON: u16 = 0x5555;
// const ON_OFF: u16 = 0xaaaa;
// const STRIPE: u16 = 0x00FF;
// const FRODO_HAMMER: u16 = 0x0100;
const BLAST: u16 = u16::MAX;

const PATTERN: u16 = BLAST;
const INIT_PATTERN: u16 = 0x0;

fn rowhammer(above_page: *const u8, below_page: *const u8) {
    for _ in 0..NO_OF_READS {
        unsafe {
            _mm_clflush(above_page);
            above_page.read_volatile();
            _mm_clflush(below_page);
            below_page.read_volatile();
        }
    }
}

fn init_row(row: &[Page], pattern: u16) {
    for page in row {
        let base_ptr = page.virt_addr as *mut u16;
        for i in 0..Consts::PAGE_SIZE / size_of_val(&pattern) {
            unsafe {
                *base_ptr.add(i) = pattern;
            }
        }
    }
}

/// Finds flipped (non-zero) bits in `row`.
///
/// # Returns
fn find_flips(page: &Page) -> [u64; Consts::MAX_BITS] {
    let mut flips = [0; Consts::MAX_BITS];
    let base_ptr = page.virt_addr as *const u16;
    for i in 0..Consts::PAGE_SIZE / 2 {
        unsafe {
            let ptr = base_ptr.add(i);
            _mm_clflush(ptr as *const u8);
            for bit in 0..size_of_val(&INIT_PATTERN) * 8 {
                flips[bit] += (((*ptr >> bit) & 1) ^ ((INIT_PATTERN >> bit) & 1)) as u64;
            }
        }
    }
    flips
}

fn hammer_all_reachable_pages(
    mmap: &mut MmapMut,
    _cores: u8,
    dimms: u8,
    bridge: Bridge,
) -> ProcResult<()> {
    let row_size = 128 * 1024 * dimms as usize;

    println!("Collecting all pages in all rows...");
    let pages_by_row = collect_pages_by_row(mmap, row_size)?;

    // let mut rng = rand::thread_rng();
    let mut total_flips = 0;
    let mut rows_tested: u32 = 0;

    if pages_by_row.len() < 3 {
        eprintln!(
            "[!] Can't hammer rows - only got {} rows total. Make sure you're running as sudo!",
            pages_by_row.len()
        );
        return Ok(());
    }

    println!("Starting rowhammer test...");

    let mut row_data = Vec::new();

    let mut rng = rand::thread_rng();
    'main: for _ in 0..pages_by_row.len() - 2 {
        let above_row_index = rng.gen::<usize>() % (pages_by_row.len() - 2);
        let target_row_index = above_row_index + 1;
        let below_row_index = above_row_index + 2;

        let above_row = &pages_by_row[above_row_index];
        let target_row = &pages_by_row[target_row_index];
        let below_row = &pages_by_row[below_row_index];

        for i in 0..3 {
            if pages_by_row[above_row_index + i].len() != row_size as usize / Consts::PAGE_SIZE {
                // eprintln!(
                //     "[!] Can't hammer row {target_row_index} - only got {} (of {}) pages from row {}.",
                //     pages_by_row[above_row_index + i].len(),
                //     row_size as usize / Consts::PAGE_SIZE,
                //     above_row_index + i,
                // );
                continue 'main;
            }
        }

        // Initialize rows (above and below get aggressor pattern, i.e. 0b0101, and target row gets zeroed)
        init_row(&above_row[..], PATTERN);
        init_row(&target_row[..], INIT_PATTERN);
        init_row(&below_row[..], PATTERN);

        // Collect a list of addresses that are in the same bank
        let mut banks = Vec::new();
        let mut virt_addrs = Vec::new();
        for above_page in above_row {
            for below_page in below_row {
                let above_bank = above_page.bank_index(bridge, dimms);
                let below_bank = below_page.bank_index(bridge, dimms);
                if above_bank == below_bank && !banks.contains(&above_bank) {
                    banks.push(above_bank);
                    virt_addrs.push((
                        above_page.virt_addr as *const u8,
                        below_page.virt_addr as *const u8,
                    ));
                }
            }
        }

        let before = Instant::now();

        // RELEASE THE BEAST
        for (above_va, below_va) in virt_addrs {
            rowhammer(above_va, below_va);
        }

        rows_tested += 1;
        println!(
            "Hammering row {} took {:.2?} seconds",
            target_row_index,
            before.elapsed(),
        );

        // Count the number of flipped bits in the target row after each test
        let mut target_row = target_row.clone();
        for ((above_page, target_page), below_page) in
            above_row.into_iter().zip(&mut target_row).zip(below_row)
        {
            let flips = find_flips(target_page);
            match target_page.data {
                Some(ref mut data) => {
                    for (old_flip, new_flip) in data.flips.iter_mut().zip(flips) {
                        *old_flip += new_flip;
                    }
                }
                None => {
                    target_page.data = Some(PageData::new(&above_page, &below_page, flips));
                }
            }
            total_flips += flips.iter().sum::<u64>();
        }

        println!("\tTarget page\tAbove PFN\tBelow PFN\tFlips\tFlipped bits");
        for page in &target_row {
            let flips = page.data.as_ref().unwrap().flips;
            let flip_sum = flips.iter().sum::<u64>();
            if flip_sum > 0 {
                let data = page.data.as_ref().unwrap();
                println!(
                    ">\t{:#x}\t{:#x}\t{:#x}\t{}\t{:?}",
                    page.pfn, data.above_pfn as usize, data.below_pfn as usize, flip_sum, flips,
                );
            }
        }
        row_data.push(target_row);

        let pages_tested = rows_tested * (row_size / Consts::PAGE_SIZE) as u32;
        println!(
            "\nSo far: {:.2} flips per page ({:.2} per row, {} flips total over {} pages tested)\n",
            total_flips as f64 / pages_tested as f64,
            total_flips as f64 / rows_tested as f64,
            total_flips,
            pages_tested,
        );
    }
    println!("Done!");
    Ok(())
}

pub(crate) fn main(fraction_of_phys_memory: f64, cores: u8, dimms: u8, bridge: Bridge) {
    println!("Setting up memory mapping...");
    let mut mmap = setup_mapping(fraction_of_phys_memory);
    hammer_all_reachable_pages(&mut mmap, cores, dimms, bridge).unwrap();
}
