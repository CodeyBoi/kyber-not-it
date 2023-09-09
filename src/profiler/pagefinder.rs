#![allow(dead_code)]
#![allow(unused_variables)]

use procfs::process::Process;

use crate::profiler::utils::{self, Consts};

struct PageCandidate {
    page_number: u32,
    above_page: u32,
    below_page: u32,

    bit_positions: [i16; Consts::MAX_BITS],
    score: i16,
    total_bit_flips: i16,
}

struct PageData {
    data: [u16; Consts::PAGE_SIZE / 2],
    above_data: [u16; Consts::PAGE_SIZE / 2],
    below_data: [u16; Consts::PAGE_SIZE / 2],
}

fn find_page_candidate(pages: &[PageCandidate], page_nbr: u32) -> Option<&PageCandidate> {
    pages.iter().find(|page| page.page_number == page_nbr)
}

pub(crate) fn some_stuff(virtual_address: u8) -> u64 {
    let process = Process::myself().expect("Failed to read process");
    let maps = process.maps().expect("Failed to read process memory maps");
    let mut pmap = process
        .pagemap()
        .expect("Failed to fetch pagemap of process");

    println!("Process: {:#?}", process);
    println!("Maps: {:#?}", maps);

    for m in maps.memory_maps {
        if let Ok(page_frame_number) = utils::get_page_frame_number(&mut pmap, m.address.0 as usize)
        {
            let phys_addr = utils::get_phys_addr(&mut pmap, m.address.0 as usize)
                .expect("Couldnt get phys address");
            println!(" PFN: {}\tPHYS: {}", page_frame_number, phys_addr);
        } else {
            println!("Found nothing for {}", m.address.0);
        }
        let page_info = pmap.get_info((m.address.0 / Consts::PAGE_SIZE as u64) as usize);

        //println!("GOT: {},\tPI: {:?}", page_frame_number, page_info);
    }

    virtual_address as u64
}
