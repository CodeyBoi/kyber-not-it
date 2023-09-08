#![allow(dead_code)]
#![allow(unused_variables)]

use procfs::{process::{PageInfo, PageMap}, ProcResult};
use sysinfo::{System, SystemExt};
use memmap2::{MmapMut, MmapOptions};

pub(crate) struct Consts;
impl Consts {
    pub(crate) const MAX_BITS: usize = 16;
    pub(crate) const PAGE_SIZE: usize = 0x1000;
}

pub(crate) fn get_phys_memory_size() -> u64 {
    let sys = System::new_all();
    sys.total_memory()
}

unsafe fn fill_memory (victim_va: *mut u8, above_va: *mut u8, below_va: *mut u8) {
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

pub(crate) fn get_page_frame_number(pagemap: &mut PageMap, virtual_addr: usize) -> ProcResult<u64> {
    match pagemap.get_info(virtual_addr / Consts::PAGE_SIZE as usize)? {
        PageInfo::MemoryPage(mempage) => {
            //println!("FLAGS: {:#?}", mempage);
            Ok(mempage.get_page_frame_number().0)
        },
        PageInfo::SwapPage(_) => unimplemented!("Swap pages are not implemented"),
    }
}


pub(crate) fn get_phys_addr(pagemap: &mut PageMap, virtual_addr: usize) -> ProcResult<u64> {
    let pfn = get_page_frame_number(pagemap, virtual_addr)?;
    // Physical address of frame is page_frame_number * page_size + offset
    Ok((pfn * Consts::PAGE_SIZE as u64) | (virtual_addr & (0x1000 - 1)) as u64)
}
