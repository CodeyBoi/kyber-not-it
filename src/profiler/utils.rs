use procfs::{process::{PageInfo, PageMap}, ProcResult};
use sysinfo::{System, SystemExt};

pub(crate) struct Consts;
impl Consts {
    pub(crate) const MAX_BITS: usize = 16;
    pub(crate) const PAGE_SIZE: usize = 0x1000;
}

pub(crate) fn get_phys_memory_size() -> u64 {
    let sys = System::new_all();
    sys.total_memory()
}

pub(crate) fn get_page_frame_number(pagemap: &mut PageMap, virtual_addr: usize) -> ProcResult<u64> {
    match pagemap.get_info(virtual_addr / Consts::PAGE_SIZE as usize)? {
        PageInfo::MemoryPage(mempage) => Ok(mempage.get_page_frame_number().0),
        PageInfo::SwapPage(_) => unimplemented!("Swap pages are not implemented"),
    }
}

pub(crate) fn get_phys_addr(pagemap: &mut PageMap, virtual_addr: usize) -> ProcResult<u64> {
    let pfn = get_page_frame_number(pagemap, virtual_addr)?;
    // Physical address of frame is page_frame_number * page_size + offset
    Ok((pfn * Consts::PAGE_SIZE as u64) | (virtual_addr & (0x1000 - 1)) as u64)
}
