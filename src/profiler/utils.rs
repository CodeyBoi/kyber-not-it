#![allow(dead_code)]
#![allow(unused_variables)]

use std::{
    cell::RefCell,
    ops::{Range, RangeFull},
};

use memmap2::{MmapMut, MmapOptions};
use procfs::{
    process::{PageInfo, PageMap},
    ProcResult,
};
use sysinfo::{System, SystemExt};

use crate::Bridge;

pub(crate) struct Consts;
impl Consts {
    pub(crate) const MAX_BITS: usize = 16;
    pub(crate) const PAGE_SIZE: usize = 0x1000;
}

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
    pub(crate) above_virt_addr: *mut u8,
    pub(crate) above_pfn: u64,
    pub(crate) below_virt_addr: *mut u8,
    pub(crate) below_pfn: u64,
    pub(crate) flips: [u64; Consts::MAX_BITS],
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
        (self.pfn as usize * Consts::PAGE_SIZE) as *mut u8
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
        out
    }
}

impl PageData {
    pub(crate) fn new(above: &Page, below: &Page, flips: [u64; Consts::MAX_BITS]) -> Self {
        Self {
            above_virt_addr: above.virt_addr,
            above_pfn: above.pfn,
            below_virt_addr: below.virt_addr,
            below_pfn: below.pfn,
            flips,
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
    let mem_size = Consts::PAGE_SIZE * 2_usize.pow(order);
    let mut mmap = MmapOptions::new()
        .len(mem_size)
        .populate()
        .map_anon()
        .expect("Failed to setup memory map over block");

    let ptr = mmap.as_mut_ptr();
    for offset in (0..mmap.len()).step_by(Consts::PAGE_SIZE) {
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

pub(crate) unsafe fn fill_memory(victim_va: *mut u8, above_va: *mut u8, below_va: *mut u8) {
    unsafe {
        std::ptr::write_bytes(victim_va, 0x00, Consts::PAGE_SIZE);
    }

    let lower_bits: u8 = 0x00;
    let upper_bits: u8 = 0x01;

    for index in 0..Consts::PAGE_SIZE {
        unsafe {
            let above_byte = above_va.add(index);
            let below_byte = below_va.add(index);

            if index % 2 == 0 {
                // Set the bytes at aboveVA and belowVA to lowerBits
                *above_byte = lower_bits;
                *below_byte = lower_bits;
            } else {
                // Set the bytes at aboveVA and belowVA to upperBits
                *above_byte = upper_bits;
                *below_byte = upper_bits;
            }
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

    let ptr = mmap.as_mut_ptr();
    for offset in (0..mmap.len()).step_by(Consts::PAGE_SIZE) {
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
    match pagemap.get_info(virtual_addr as usize / Consts::PAGE_SIZE)? {
        PageInfo::MemoryPage(mempage) => {
            //println!("FLAGS: {:#?}", mempage);
            Ok(mempage.get_page_frame_number().0)
        }
        PageInfo::SwapPage(_) => Err(procfs::ProcError::NotFound(None)),
    }
}

pub(crate) fn collect_pages_by_row(mmap: &mut MmapMut, pagemap: &mut PageMap, row_size: usize) -> Vec<Row> {
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

pub(crate) fn get_phys_addr(pagemap: &mut PageMap, virtual_addr: *const u8) -> ProcResult<u64> {
    let pfn = get_page_frame_number(pagemap, virtual_addr)?;
    // Physical address of frame is page_frame_number * page_size + offset
    Ok((pfn * Consts::PAGE_SIZE as u64) | (virtual_addr as usize & (0x1000 - 1)) as u64)
}
