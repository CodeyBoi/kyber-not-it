#![allow(dead_code)]
#![allow(unused_variables)]

use std::{fs::{File, create_dir}, io::Write};

use procfs::process::Process;

use crate::profiler::utils::{self, Consts, Page};


pub(crate) struct PageCandidate {
    target_page: Page,
    above_page: Page,
    below_page: Page,

    score: u32,
}

impl PageCandidate {
    pub(crate) fn new(target_page: Page, above_page: Page, below_page: Page) -> Self {
        let target_flips = target_page.data.as_ref().unwrap().flips;

        Self {
            target_page,
            above_page,
            below_page,

            score: Self::calculate_score(&target_flips),
        }
    }

    fn calculate_score(flips: &[u8]) -> u32 {
        let position_bonus = 10;
        let score = (flips[8] + 1) as u32 * position_bonus;
    
        score
    }
}

fn calculate_risk_score(page: &Page) -> u32 {
    let mut risk_score = 0;
    let max_position = 9;

    let target_flips = page.data.as_ref().unwrap().flips;
    for (i, bit) in target_flips[max_position..].iter().enumerate() {
        risk_score += i as u8 * bit;
    }

    risk_score as u32
}

fn count_256_flip(page_candidate: &PageCandidate) {
    let target_flips = page_candidate.target_page.data.as_ref().unwrap().flips;

    println!("Target Page {:#x?} has {} 256 flips", page_candidate.target_page.pfn, target_flips[8]);
}

fn find_page_candidate(pages: &[PageCandidate], page_nbr: u64) -> Option<&PageCandidate> {
    pages.iter().find(|page_candidate| page_candidate.target_page.pfn == page_nbr)
}

pub(crate) fn output_page(page_candidate: &PageCandidate) {
    let mut path = std::env::current_dir().unwrap();

    if !path.join("data").exists() {
        create_dir(path.join("data")).unwrap();
    }

    path.push(format!("data/V_{}", page_candidate.target_page.virt_addr as u64));
    path.set_extension("out");
    println!("PATH: {:#?}", path);

    let mut file = File::create(path).unwrap();

    file.write_all(format!("Page: {}, addr: {}\nAbove: {}, addr: {}, Below: {}, addr: {}\n",
                            page_candidate.target_page.pfn,
                            page_candidate.target_page.virt_addr as u64,
                            page_candidate.above_page.pfn,
                            page_candidate.above_page.virt_addr as u64,
                            page_candidate.below_page.pfn,
                            page_candidate.below_page.virt_addr as u64,
                        ).as_bytes()).unwrap();

    file.write_all(format!(
                            "Score: {}\nbit flips on halfword index:\n", 
                            page_candidate.score
                        ).as_bytes()).unwrap();

    let target_flips = page_candidate.target_page.data.as_ref().unwrap().flips;

    for i in 0..Consts::MAX_BITS {
        file.write_all(format!("{}\t", i).as_bytes()).unwrap();
        if i == Consts::MAX_BITS - 1 {
            file.write(b"\n").unwrap();
        }
    } 

    for value in target_flips {
        file.write_all(format!("{value}\t").as_bytes()).unwrap();
    }
}

fn get_candidate_pages(pages: &[Page]) -> Vec<PageCandidate> {
    let mut page_candidates = Vec::new();

    //for page in pages {
    //    let above_page = page.above_page.unwrap();
    //    let below_page = page.below_page.unwrap();

    //    let page_candidate = PageCandidate::new(*page, above_page, below_page);
    //    page_candidates.push(page_candidate);
    //}

    page_candidates
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
        if let Ok(page_frame_number) =
            utils::get_page_frame_number(&mut pmap, m.address.0 as *const u8)
        {
            let phys_addr = utils::get_phys_addr(&mut pmap, m.address.0 as *const u8)
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
