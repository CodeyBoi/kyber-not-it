use std::{mem::size_of_val, time::Instant};

use memmap2::MmapMut;
use procfs::ProcResult;
use rand::seq::SliceRandom;

use crate::{
    profiler::utils::{
        collect_pages_by_row, find_flips, rowhammer, setup_mapping, Consts, Page, PageData,
    },
    Bridge,
};

// const OFF_ON: u16 = 0x5555;
// const ON_OFF: u16 = 0xaaaa;
// const STRIPE: u16 = 0x00FF;
// const FRODO_HAMMER: u16 = 0x0100;
const BLAST: u16 = u16::MAX;
const INIT_PATTERN: u16 = 0x0;
const PATTERN: u16 = BLAST;

/// Initializes all halfword (16-bit) values in `row` to `pattern`.
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

/// Finds two pages in `row` which are in the same bank as `target`.
///
/// # Returns
/// A tuple of two pages in the same bank as `target`, sorted by their page frame number (PFN).
fn find_pfns_in_same_bank<'a>(
    target: &Page,
    row: &'a [Page],
    bridge: Bridge,
    dimms: u8,
) -> (&'a Page, &'a Page) {
    let mut pages = (None, None);
    for page in row {
        if page.bank_index(bridge, dimms) == target.bank_index(bridge, dimms) {
            if pages.0.is_none() {
                pages.0 = Some(page);
            } else {
                pages.1 = Some(page);
                break;
            }
        }
    }
    match pages {
        (Some(p1), Some(p2)) => {
            if p1.pfn < p2.pfn {
                (p1, p2)
            } else {
                (p2, p1)
            }
        }
        _ => panic!("Couldn't find two pages in the same bank!"),
    }
}

/// Collects all pages in `pages` by their bank index.
///
/// # Returns
/// A vector of vectors of pages, where each inner vector contains all pages in the same bank. The
/// index of the inner vector is the bank index.
fn get_pages_by_bank<'a>(pages: &'a [Page], bridge: Bridge, dimms: u8) -> Vec<Vec<&'a Page>> {
    let mut pages_by_bank = Vec::new();
    for page in pages {
        let bank = page.bank_index(bridge, dimms) as usize;
        if pages_by_bank.len() <= bank {
            pages_by_bank.resize(bank + 1, Vec::new());
        }
        pages_by_bank[bank].push(page);
    }
    pages_by_bank
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

    // If we don't have at least 3 rows we can't hammer rows. The reason for not getting rows is probably
    // that we're not running as root.
    if pages_by_row.len() < 3 {
        eprintln!(
            "[!] Can't hammer rows - only got {} rows total. Make sure you're running as sudo!",
            pages_by_row.len()
        );
        return Ok(());
    }

    println!("Starting rowhammer test...");

    // Initializing loop variables
    let mut total_flips = 0;
    let mut rows_skipped = 0;
    let mut rows_tested = 0;

    // Shuffle the row indices so we hammer the rows in a random order
    let mut rng = rand::thread_rng();
    let mut indices = (0..pages_by_row.len() - 2).collect::<Vec<_>>();
    indices.shuffle(&mut rng);

    'main: for above_row_index in indices {
        let target_row_index = above_row_index + 1;
        let below_row_index = above_row_index + 2;

        let above_row = &pages_by_row[above_row_index];
        let target_row = &pages_by_row[target_row_index];
        let below_row = &pages_by_row[below_row_index];

        // If any of the rows are not full we can't hammer them, so continue to the next iteration
        for i in 0..3 {
            if pages_by_row[above_row_index + i].len() != row_size as usize / Consts::PAGE_SIZE {
                rows_skipped += 1;
                continue 'main;
            }
        }

        // Initialize rows (above and below get aggressor pattern, i.e. 0b0101, and target row gets zeroed)
        init_row(&above_row[..], PATTERN);
        init_row(&target_row[..], INIT_PATTERN);
        init_row(&below_row[..], PATTERN);

        // Collect a list of addresses that are in the same bank
        let above_pages_by_bank = get_pages_by_bank(&above_row[..], bridge, dimms);
        let below_pages_by_bank = get_pages_by_bank(&below_row[..], bridge, dimms);

        // RELEASE THE BEAST
        let before = Instant::now();
        for (above, below) in above_pages_by_bank
            .iter()
            .map(|p| p.first())
            .zip(below_pages_by_bank.iter().map(|p| p.first()))
        {
            // We only need to hammer one page in each bank as each row access hammers the entire row,
            // so we pick the first one
            match (above, below) {
                (Some(a), Some(b)) => {
                    rowhammer(a.virt_addr, b.virt_addr);
                }
                _ => continue,
            }
        }
        rows_tested += 1;

        println!(
            "Hammering row {} took {:.2?} seconds\n",
            target_row_index,
            before.elapsed(),
        );

        // Count the number of flipped bits in the target row after each test and sets which are above and below pages
        let mut target_row = target_row.clone();
        for target_page in &mut target_row {
            let above_pages = find_pfns_in_same_bank(target_page, &above_row[..], bridge, dimms);
            let below_pages = find_pfns_in_same_bank(target_page, &below_row[..], bridge, dimms);

            let flips = find_flips(target_page, INIT_PATTERN);
            match target_page.data {
                Some(ref mut data) => {
                    for (old_flip, new_flip) in data.flips.iter_mut().zip(flips) {
                        *old_flip += new_flip;
                    }
                }
                None => {
                    target_page.data = Some(PageData::new(
                        (above_pages.0.pfn, above_pages.1.pfn),
                        (below_pages.0.pfn, below_pages.1.pfn),
                        flips,
                    ));
                }
            }
            total_flips += flips.iter().sum::<u64>();
        }

        let width = 12;
        println!(
            "\t{:<width$}{:<width$}{:<width$}{:<width$}{:<width$}{:<7}{}",
            "Page", "aPFN1", "aPFN2", "bPFN1", "bPFN2", "Flips", "Flipped bits"
        );
        for page in &target_row {
            let flips = page.data.as_ref().unwrap().flips;
            let flip_sum = flips.iter().sum::<u64>();
            if flip_sum > 0 {
                let data = page.data.as_ref().unwrap();
                println!(
                    ">\t{:<#width$x}{:<#width$x}{:<#width$x}{:<#width$x}{:<#width$x}{:<7}{:?}",
                    page.pfn,
                    data.above_pfns.0,
                    data.above_pfns.1,
                    data.below_pfns.0,
                    data.below_pfns.1,
                    flip_sum,
                    flips,
                );
            }
        }

        let pages_tested = rows_tested * row_size / Consts::PAGE_SIZE;
        println!(
            "\nSo far: {:.2} flips per page ({:.2} per row, {} flips total over {} pages tested)",
            total_flips as f64 / pages_tested as f64,
            total_flips as f64 / rows_tested as f64,
            total_flips,
            pages_tested,
        );
        let rows_analyzed = rows_tested + rows_skipped;
        println!(
            "        {:.2}% of allocated memory analyzed ({:.2}% tested, {:.2}% skipped)\n",
            rows_analyzed as f64 * 100.0 / pages_by_row.len() as f64,
            rows_tested as f64 / rows_analyzed as f64 * 100.0,
            rows_skipped as f64 / rows_analyzed as f64 * 100.0,
        )
    }
    println!("Done!");
    Ok(())
}

pub(crate) fn main(fraction_of_phys_memory: f64, cores: u8, dimms: u8, bridge: Bridge) {
    println!("Setting up memory mapping...");
    let mut mmap = setup_mapping(fraction_of_phys_memory);
    hammer_all_reachable_pages(&mut mmap, cores, dimms, bridge).unwrap();
}
