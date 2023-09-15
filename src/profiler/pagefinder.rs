#![allow(dead_code)]
#![allow(unused_variables)]

use std::{
    fs::{create_dir, File},
    io::{self, BufRead, Write},
};

use procfs::process::Process;

use crate::profiler::utils::{
                     self,
                     setup_mapping,
                     collect_pages_by_row,
                     Consts,
                     Page,
                     Row,
};

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

fn find_page(pages: &[Page], page_nbr: u64) -> Option<&Page> {
    pages.iter().find(|page| page.pfn == page_nbr)
}

fn setup_page_candidate(pages_by_row: Vec<Row>, page_numbers: &[u64; 3]) -> Result<PageCandidate, &'static str> {

    // Get page frame numbers
    let target_pfn = page_numbers[0];
    let above_pfn =  page_numbers[1];
    let below_pfn = page_numbers[2];

    // Find the rows that contains the target pages
    for index in 0..pages_by_row.len() - 2 {
        let above_row_index = index;
        let target_row_index = index + 1;
        let below_row_index = index + 2;

        let above_row = &pages_by_row[above_row_index];
        let target_row = &pages_by_row[target_row_index];
        let below_row = &pages_by_row[below_row_index];

        let Some(target_page) = find_page(&target_row[..], target_pfn) else {
            println!("Target page not found in row, skipping row");
            continue;
        };

        let Some(above_page) = find_page(&above_row[..], above_pfn) else {
            println!("Above page not found in row, skipping row");
            continue;
        };

        let Some(below_page) = find_page(&below_row[..], below_pfn) else {
            println!("Below page not found in row, skipping row");
            continue;
        };

        // If pages are found, create a PageCandidate
        let target_page = Page::new(target_page.virt_addr, target_pfn);
        let above_page = Page::new(above_page.virt_addr, above_pfn);
        let below_page = Page::new(below_page.virt_addr, below_pfn);

        let page_candidate = PageCandidate::new(target_page, above_page, below_page);

        return Ok(page_candidate);
    }

    Err("Could not find page candidate in current mapping, remapping!!!")
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
fn get_candidate_pages(pages_by_row: Vec<Row>) -> Result<Vec<PageCandidate>, &'static str> {
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

            let target_page_nbr = split_line[0].parse::<u64>().unwrap();
            let above_page_nbr = split_line[1].parse::<u64>().unwrap();
            let below_page_nbr = split_line[2].parse::<u64>().unwrap();

            let page_numbers = [target_page_nbr, above_page_nbr, below_page_nbr];

            let page_candidate = setup_page_candidate(pages_by_row, &page_numbers)?;
            page_candidates.push(page_candidate);
        }
    }
    println!("Time: {:#?}", start.elapsed());

    Ok(page_candidates)
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

pub(crate) fn main(dimms: u8) {
    let fraction_of_phys_memory = 0.1;
    let row_size = 128 * 1024 * dimms as usize;

    let (pages_by_row, candidates) = loop {
        println!("Setting up memory mapping with {} of physical memory", fraction_of_phys_memory);
        let mut mmap = setup_mapping(fraction_of_phys_memory);

        println!("Collecting pages from mapping...");
        let pages_by_row = collect_pages_by_row(&mut mmap, row_size);

        let pages_by_row = match pages_by_row {
            Ok(pages_by_row) => {
                pages_by_row
            }
            Err(e) => {
                println!("Couldn't collect pages from mapping, got {:#?}", e);
                fraction_of_phys_memory += 0.1;
                continue;
            }
        };

        println!("Finding candidate pages...");
        let candidates = get_candidate_pages(pages_by_row);
        let candidates = match candidates {
            Ok(candidates) => {
                candidates
            }
            Err(e) => {
                println!("Couldn't find candidate pages, got {:#?}", e);
                fraction_of_phys_memory += 0.1;
                continue;
            }
        };

        println!("Found {} candidate pages", candidates.len());
        break (pages_by_row, candidates);
    };
}
