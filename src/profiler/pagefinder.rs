#![allow(dead_code)]
#![allow(unused_variables)]

use std::{
    fs::{create_dir, File},
    io::{self, BufRead, Write},
};

use procfs::process::Process;

use crate::profiler::utils::{self, setup_mapping, Consts, Page};

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

    /// Calculates the score of the PageCandidate
    fn calculate_score(flips: &[u64]) -> u32 {
        let position_bonus = 10;
        let score = (flips[8] + 1) as u32 * position_bonus;

        score
    }
}

/// Calculates the risk score of a page based on the uppermost 8 bits per halfword
fn calculate_risk_score(page: &Page) -> u32 {
    let mut risk_score = 0;
    let max_position = 9;

    let target_flips = page.data.as_ref().unwrap().flips;
    for (i, bit) in target_flips[max_position..].iter().enumerate() {
        risk_score += i as u64 * bit;
    }

    risk_score as u32
}

/// Print the number of 256 flips on the PageCandidate
fn count_256_flip(page_candidate: &PageCandidate) {
    let target_flips = page_candidate.target_page.data.as_ref().unwrap().flips;

    println!(
        "Target Page {:#x?} has {} 256 flips",
        page_candidate.target_page.pfn, target_flips[8]
    );
}

fn find_page_candidate(pages: &[PageCandidate], page_nbr: u64) -> Option<&PageCandidate> {
    pages
        .iter()
        .find(|page_candidate| page_candidate.target_page.pfn == page_nbr)
}

fn setup_page_candidates() {
    let fraction_of_phys_memory = 0.8;

    let mut mmap = setup_mapping(fraction_of_phys_memory);

    //collect_pages_by_row(&mut mmap, pagemap, row_size);
}

/// Output the PageCandidate to a file
fn output_page(page_candidate: &PageCandidate) -> io::Result<()> {
    let mut path = std::env::current_dir()?;

    if !path.join("data").exists() {
        create_dir(path.join("data"))?;
    }

    path.push(format!(
        "data/V_{}",
        page_candidate.target_page.virt_addr as u64
    ));
    path.set_extension("out");
    println!("PATH: {:#?}", path);

    let mut file = File::create(path)?;

    file.write_all(
        format!(
            "Page: {}, addr: {}\nAbove: {}, addr: {}, Below: {}, addr: {}\n",
            page_candidate.target_page.pfn,
            page_candidate.target_page.virt_addr as u64,
            page_candidate.above_page.pfn,
            page_candidate.above_page.virt_addr as u64,
            page_candidate.below_page.pfn,
            page_candidate.below_page.virt_addr as u64,
        )
        .as_bytes(),
    )?;

    file.write_all(
        format!(
            "Score: {}\nbit flips on halfword index:\n",
            page_candidate.score
        )
        .as_bytes(),
    )?;

    let target_flips = page_candidate.target_page.data.as_ref().unwrap().flips;

    for i in 0..Consts::MAX_BITS {
        file.write_all(format!("{}\t", i).as_bytes())?;
        if i == Consts::MAX_BITS - 1 {
            file.write(b"\n")?;
        }
    }

    for value in target_flips {
        file.write_all(format!("{value}\t").as_bytes())?;
    }

    Ok(())
}

/// Read the flips.txt file and return a vector of potential exploitable pages
fn get_candidate_pages(pages: &[Page]) -> Vec<PageCandidate> {
    let mut page_candidates = Vec::new();

    let mut path = std::env::current_dir().unwrap();
    let file_name = "flips.txt";
    path.push(file_name);

    let file = File::open(path).expect("Failed to open file {path}");

    let lines = io::BufReader::new(file).lines();
    let start = std::time::Instant::now();

    for line in lines {
        if let Ok(s) = line {
            // Dont read line unless it starts with '>'
            if !s.starts_with(">") {
                continue;
            }

            let str = s.as_str();

            let start_flips = str.find("[").unwrap();
            let end_flips = str.find("]").unwrap_or(str.len());

            let flips = &str[start_flips + 1..end_flips];
            let flips = flips
                .split(",")
                .map(|s| s.trim().parse::<u32>().unwrap())
                .collect::<Vec<_>>();

            let good_sum = flips[8];
            let risk_sum = flips[9..]
                .iter()
                .enumerate()
                .fold(0, |acc, (i, bit)| acc + i as u32 * bit);

            let split_line = str[1..].split_whitespace().collect::<Vec<_>>();

            if risk_sum > 0 || good_sum < 3 {
                println!(
                    "Skipping Page {}, got risk: {}, and 256 flips {}",
                    split_line[1], risk_sum, good_sum
                );
                continue;
            }

            // Create PageCandidate from the page and add it to the vector
            // Take the first hex value from split_line and parse it to u64
            let page_nbr = split_line[0].parse::<u64>().unwrap();
            let above_page_nbr = split_line[1].parse::<u64>().unwrap();
            let below_page_nbr = split_line[2].parse::<u64>().unwrap();
        }
    }
    println!("Time: {:#?}", start.elapsed());

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

pub(crate) fn main() {
    let pages = [Page::new(0x001 as *mut u8, 1)];
    get_candidate_pages(&pages);
}
