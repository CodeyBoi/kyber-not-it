use std::{
    arch::x86_64::{_mm_clflush, _mm_lfence, _mm_mfence},
    cell::RefCell,
    mem::size_of_val,
    ops::{Range, RangeFull},
};

use memmap2::{MmapMut, MmapOptions};
use procfs::{
    process::{PageInfo, PageMap, Process},
    ProcResult,
};
use sysinfo::{System, SystemExt};

use crate::Bridge;

pub(crate) const MAX_BITS: usize = 16;
pub(crate) const PAGE_SIZE: usize = 0x1000;
pub(crate) const NO_OF_READS: usize = 3_000_000;

#[derive(Clone, Debug)]
pub(crate) struct Row {
    pages: Vec<Page>,
    pub(crate) presumed_index: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Page {
    pub(crate) virt_addr: *mut u8,
    pub(crate) pfn: u64,
    pub(crate) data: Option<PageData>,
    bank_index: RefCell<Option<u8>>,
}

#[derive(Debug, Clone)]
pub(crate) struct PageData {
    pub(crate) above_pfns: (u64, u64),
    pub(crate) below_pfns: (u64, u64),
    pub(crate) flips: [u64; MAX_BITS],
    pub(crate) flip_offsets: Vec<usize>,
}

impl Row {
    pub(crate) fn new(presumed_index: usize) -> Self {
        Self {
            pages: Vec::new(),
            presumed_index,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.pages.len()
    }

    pub(crate) fn push(&mut self, page: Page) {
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

impl<'a> IntoIterator for &'a Row {
    type Item = &'a Page;
    type IntoIter = std::slice::Iter<'a, Page>;
    fn into_iter(self) -> Self::IntoIter {
        self.pages.iter()
    }
}

impl<'a> IntoIterator for &'a mut Row {
    type Item = &'a mut Page;
    type IntoIter = std::slice::IterMut<'a, Page>;
    fn into_iter(self) -> Self::IntoIter {
        self.pages.iter_mut()
    }
}

impl Page {
    pub(crate) fn new(virt_addr: *mut u8, pfn: u64) -> Self {
        Self {
            virt_addr,
            pfn,
            bank_index: RefCell::new(None),
            data: None,
        }
    }

    pub(crate) fn phys_addr(&self) -> *mut u8 {
        (self.pfn as usize * PAGE_SIZE) as *mut u8
    }

    pub(crate) fn bank_index(&self, bridge: Bridge, dimms: u8) -> u8 {
        let mut bank_index = self.bank_index.borrow_mut();
        match *bank_index {
            Some(b) => b,
            None => {
                *bank_index = Some(self.calc_bank_index(bridge, dimms));
                bank_index.expect("Something went wrong when caching bank index")
            }
        }
    }

    pub(crate) fn col(&self) -> u64 {
        let phys_addr = self.phys_addr() as u64;
        (phys_addr & ((0b1 << 7) - 0b1)) | ((phys_addr >> 8) & ((1 << 6) - 1) << 7)
    }

    fn calc_bank_index(&self, bridge: Bridge, dimms: u8) -> u8 {
        let phys_addr = self.phys_addr() as usize;
        let bank_bits = get_bank_bits(bridge);
        let bank_bits = if dimms == 2 {
            &bank_bits
        } else {
            &bank_bits[..bank_bits.len() - 1]
        };
        let mut out = 0u8;
        for bits in bank_bits {
            for bit in bits {
                out ^= ((phys_addr >> bit) & 1) as u8;
            }
            out <<= 1;
        }
        out >> 1
    }
}

impl PageData {
    pub(crate) fn new(
        above_pfns: (u64, u64),
        below_pfns: (u64, u64),
        flips: [u64; MAX_BITS],
        flip_offsets: Vec<usize>,
    ) -> Self {
        Self {
            above_pfns,
            below_pfns,
            flips,
            flip_offsets,
        }
    }
}

pub(crate) fn get_bank_bits(bridge: Bridge) -> Vec<Vec<u8>> {
    match bridge {
        Bridge::Haswell => vec![
            vec![14, 18],
            vec![15, 19],
            vec![16, 20],
            vec![17, 21],
            vec![7, 8, 9, 12, 13, 18, 19],
        ],
        Bridge::Sandy => vec![
            vec![14, 18],
            vec![15, 19],
            vec![16, 20],
            vec![17, 21],
            vec![17, 21],
            vec![6],
        ],
    }
}

pub(crate) fn get_block_by_order(order: u32) -> MmapMut {
    let mem_size = PAGE_SIZE * 2_usize.pow(order);
    let mut mmap = MmapOptions::new()
        .len(mem_size)
        .populate()
        .map_anon()
        .expect("Failed to setup memory map over block");

    let ptr = mmap.as_mut_ptr();
    for offset in (0..mmap.len()).step_by(PAGE_SIZE) {
        unsafe {
            *ptr.add(offset) = 1 + offset as u8;
        }
    }
    mmap
}

pub(crate) fn get_phys_memory_size() -> u64 {
    let sys = System::new_all();
    sys.total_memory()
}

/// Finds flipped (non-zero) bits in `page`.
///
/// # Returns
/// A vector of tuples containing the index of the halfword (u16) and the index of the
/// flipped bit in that halfword (0-15).
pub(crate) fn find_flips(page: &Page, initial_pattern: u16) -> Vec<(usize, usize)> {
    let mut flips = Vec::new();
    let base_ptr = page.virt_addr as *const u16;
    for i in 0..PAGE_SIZE / 2 {
        unsafe {
            let ptr = base_ptr.add(i);
            _mm_clflush(ptr as *const u8);
            for bit in 0..size_of_val(&initial_pattern) * 8 {
                if (((*ptr >> bit) & 1) ^ ((initial_pattern >> bit) & 1)) == 1 {
                    flips.push((i, bit));
                }
            }
        }
    }
    flips
}

/// Counts the number of flipped bits in `page` by bit.
///
/// # Returns
/// An array of length 16, where each index corresponds to the number of flipped bits
/// in that bit position.
/// The offsets in the page where the bits are flipped.
pub(crate) fn count_flips_by_bit(
    page: &Page,
    initial_pattern: u16,
) -> ([u64; MAX_BITS], Vec<usize>) {
    let mut flips = [0; MAX_BITS];
    let base_ptr = page.virt_addr as *const u16;
    let mut flip_offsets = Vec::new();

    for i in 0..PAGE_SIZE / 2 {
        unsafe {
            let ptr = base_ptr.add(i);
            _mm_clflush(ptr as *const u8);
            for bit in 0..size_of_val(&initial_pattern) * 8 {
                // If the bit is flipped, add the offset to the list of offsets
                let set = (((*ptr >> bit) & 1) ^ ((initial_pattern >> bit) & 1)) as u64;

                if set == 1 {
                    flip_offsets.push(i);
                    flips[bit] += set;
                }
            }
        }
    }
    (flips, flip_offsets)
}

pub(crate) unsafe fn fill_memory(victim_va: *mut u8, above_va: *mut u8, below_va: *mut u8) {
    unsafe {
        std::ptr::write_bytes(victim_va, 0x00, PAGE_SIZE);
    }

    let above_va = above_va as *mut u16;
    let below_va = below_va as *mut u16;

    let pattern = 0x0100;

    for index in 0..PAGE_SIZE / 2 {
        unsafe {
            let above = above_va.add(index);
            let below = below_va.add(index);

            *above = pattern;
            *below = pattern;
        }
    }
}

/// Setup the memory mapping used for the memory profiling
pub(crate) fn setup_mapping(fraction_of_phys_memory: f64) -> MmapMut {
    let mut mmap = MmapOptions::new()
        .len((get_phys_memory_size() as f64 * fraction_of_phys_memory) as usize)
        .populate()
        .map_anon()
        .expect("failed to setup memory mapping");

    mmap.lock().expect("failed to lock mmap to RAM.");

    let ptr = mmap.as_mut_ptr();
    for offset in (0..mmap.len()).step_by(PAGE_SIZE) {
        unsafe {
            *ptr.add(offset) = offset as u8;
        }
    }

    mmap
}

pub(crate) fn get_page_frame_number(
    pagemap: &mut PageMap,
    virtual_addr: *const u8,
) -> ProcResult<u64> {
    match pagemap.get_info(virtual_addr as usize / PAGE_SIZE)? {
        PageInfo::MemoryPage(mempage) => {
            //println!("FLAGS: {:#?}", mempage);
            Ok(mempage.get_page_frame_number().0)
        }
        PageInfo::SwapPage(_) => Err(procfs::ProcError::NotFound(None)),
    }
}

pub(crate) fn rowhammer(above_row: *const u8, below_row: *const u8, iters: usize) {
    for _ in 0..iters {
        unsafe {
            _mm_clflush(above_row);
            above_row.read_volatile();
            _mm_clflush(below_row);
            below_row.read_volatile();
        }
    }
}

pub(crate) fn rowpress(
    above_row: *const u8,
    below_row: *const u8,
    iters: usize,
    aggressor_activations: usize,
    reads: usize,
) {
    for _ in 0..iters {
        unsafe {
            _mm_lfence();
        }

        for _ in 0..aggressor_activations {
            // Read both aggressor rows in sequence
            for i in 0..reads {
                unsafe {
                    above_row.add(i).read_volatile();
                }
            }
            for i in 0..reads {
                unsafe {
                    below_row.add(i).read_volatile();
                }
            }

            for i in 0..reads {
                unsafe {
                    _mm_clflush(above_row.add(i));
                    _mm_clflush(below_row.add(i));
                }
            }

            unsafe {
                _mm_mfence();
            }
        }
    }
}

pub(crate) fn collect_pages_by_row(mmap: &mut MmapMut, row_size: usize) -> ProcResult<Vec<Row>> {
    let base_ptr = mmap.as_mut_ptr();
    let mut rows = Vec::new();
    let pagemap = &mut Process::myself()?.pagemap()?;

    for offset in (0..mmap.len()).step_by(PAGE_SIZE) {
        unsafe {
            let virtual_addr = base_ptr.add(offset);
            if let Ok(pfn) = get_page_frame_number(pagemap, virtual_addr) {
                let physical_addr = pfn as usize * PAGE_SIZE;
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
    Ok(rows)
}

pub(crate) fn get_phys_addr(pagemap: &mut PageMap, virtual_addr: *const u8) -> ProcResult<u64> {
    let pfn = get_page_frame_number(pagemap, virtual_addr)?;
    // Physical address of frame is page_frame_number * page_size + offset
    Ok((pfn * PAGE_SIZE as u64) | (virtual_addr as usize & (0x1000 - 1)) as u64)
}
