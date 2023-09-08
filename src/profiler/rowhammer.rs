#![allow(dead_code)]
#![allow(unused_variables)]

use std::{
    arch::x86_64::_mm_clflush,
    mem::size_of,
    ops::{Range, RangeFull},
    time::Instant,
};

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

const NO_OF_READS: u64 = 27 * 100 * 1000 * 4;
const STRIPE: [u64; 3] = [0x00FF00FF00FF00FF, 0, 0x00FF00FF00FF00FF];

fn get_hashes(bridge: Bridge) -> [Vec<u8>; 6] {
    match bridge {
        Bridge::Haswell => [
            vec![14, 18],
            vec![15, 19],
            vec![16, 20],
            vec![17, 21],
            vec![17, 21],
            vec![7, 8, 9, 12, 13, 18, 19],
        ],
        Bridge::Sandy => [
            vec![14, 18],
            vec![15, 19],
            vec![16, 20],
            vec![17, 21],
            vec![17, 21],
            vec![6],
        ],
    }
}

fn rowhammer(above_page: *mut u8, below_page: *mut u8) -> u64 {
    // let t0 = rdtsc();
    let mut sum = 0;
    for _ in 0..NO_OF_READS {
        for ptr in [above_page, below_page] {
            unsafe {
                // To avoid the compiler optimizing out the loop (it might or might not do this)
                sum += ptr.read_volatile() as u64;
                _mm_clflush(ptr);
            }
        }
    }
    sum
}

#[derive(Clone, Debug)]
struct Row {
    pages: Vec<Page>,
    pub(crate) presumed_index: usize,
}

impl Row {
    fn new(presumed_index: usize) -> Self {
        Self {
            pages: Vec::new(),
            presumed_index,
        }
    }
    fn len(&self) -> usize {
        self.pages.len()
    }
    fn push(&mut self, page: Page) {
        self.pages.push(page);
    }
}

impl std::ops::Index<usize> for Row {
    type Output = Page;
    fn index(&self, index: usize) -> &Self::Output {
        &self.pages[index]
    }
}

impl std::ops::Index<Range<usize>> for Row {
    type Output = [Page];
    fn index(&self, index: Range<usize>) -> &Self::Output {
        &self.pages[index.start..index.end]
    }
}

impl std::ops::Index<RangeFull> for Row {
    type Output = [Page];
    fn index(&self, _: RangeFull) -> &Self::Output {
        &self.pages[..]
    }
}

// impl Iterator for Row {
//     type Item = Page;
//     fn next(&mut self) -> Option<Self::Item> {
//         self.pages.iter().next().map(|page| *page)
//     }
// }

impl<'a> IntoIterator for &'a Row {
    type Item = &'a Page;
    type IntoIter = std::slice::Iter<'a, Page>;
    fn into_iter(self) -> Self::IntoIter {
        self.pages.iter()
    }
}

#[derive(Clone, Copy, Debug)]
struct Page {
    pub(crate) ptr: *mut u8,
    pub(crate) pfn: u64,
}

impl Page {
    fn new(ptr: *mut u8, pfn: u64) -> Self {
        Self { ptr, pfn }
    }

    fn phys_addr(&self) -> usize {
        self.pfn as usize * Consts::PAGE_SIZE
    }

    fn dram_mapping(&self, bridge: Bridge, dimms: u8) -> usize {
        let phys_addr = self.phys_addr();
        let single_dimm_shift = if dimms == 1 { 1 } else { 0 };
        let mut out = 0;
        for hash in get_hashes(bridge) {
            let mut tmp = 0;
            for h in hash {
                tmp ^= (phys_addr >> (h - single_dimm_shift)) & 1;
            }
            out = (out << 1) | tmp;
        }
        out as usize
    }
}

fn collect_pages_by_row(mmap: &mut MmapMut, pagemap: &mut PageMap, row_size: usize) -> Vec<Row> {
    let base_ptr = mmap.as_mut_ptr();
    let mut rows = Vec::new();
    for offset in (0..mmap.len()).step_by(Consts::PAGE_SIZE) {
        unsafe {
            let virtual_addr = base_ptr.add(offset);
            if let Ok(pfn) = get_page_frame_number(pagemap, virtual_addr as usize) {
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
            let ptr = page.ptr as *mut u64;
            for i in 0..Consts::PAGE_SIZE / size_of::<u64>() {
                unsafe {
                    *ptr.add(i) = pattern;
                    _mm_clflush(page.ptr);
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
                _mm_clflush(page.ptr);
                let byte = *(page.ptr).add(i);
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
    let mut pagemap = Process::myself()?.pagemap()?;
    let row_size = 128 * 1024 * dimms as usize;

    println!("Collecting all pages in all rows...");
    let pages_by_row = collect_pages_by_row(mmap, &mut pagemap, row_size);

    // let mut rng = rand::thread_rng();
    let mut total_flips = 0;
    let mut no_of_rows_tested: u32 = 0;

    'main: for above_row_index in 0..pages_by_row.len() - 2 {
        // let above_row_index = rng.gen::<usize>() % (pages_by_row.len() - 2);
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
        println!("Initializing rows {above_row_index}-{below_row_index}...");
        init_rows(rows);

        let before = Instant::now();
        for (above_row_page, below_row_page) in above_row.into_iter().zip(below_row.into_iter()) {
            let above_row_mapping = above_row_page.dram_mapping(bridge, dimms);
            let below_row_mapping = below_row_page.dram_mapping(bridge, dimms);

            if above_row_mapping != below_row_mapping {
                continue;
            }

            println!("Hammering rows {above_row_index}-{below_row_index}...");
            // RELEASE THE BEAST
            rowhammer(above_row_page.ptr, below_row_page.ptr);
        }
        println!(
            "Hammering row {target_row_index} took {:.2?} seconds",
            before.elapsed()
        );
        no_of_rows_tested += 1;

        // Count the flips in the row after hammering it
        let (flips, no_of_flips) = find_flips(&target_row[..]);
        total_flips += no_of_flips;
        if flips.is_empty() {
            println!("No flips found in row {target_row_index}.");
        } else {
            println!("Found {no_of_flips} flips in row {target_row_index}:");
        }

        for (flipped_page, bit_indices) in flips {
            let pfn = get_page_frame_number(&mut pagemap, flipped_page.ptr as usize)?;
            println!("\tpfn: {pfn}\tflipped bits at: {:?}", bit_indices);
        }

        println!(
            "So far: {:.4} flips per row ({no_of_rows_tested} rows tested, {total_flips} flips total)\n",
            total_flips as f64 / no_of_rows_tested as f64,
        );
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

        let mut rng = rand::thread_rng();
        'main: for _ in 0..pages_by_row.len() - 2 {
            let row = rng.gen::<usize>() % (pages_by_row.len() - 2);
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
                    let ptr = page.ptr as *mut u64;
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
                    let ptr = page.ptr as *mut u64;
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
