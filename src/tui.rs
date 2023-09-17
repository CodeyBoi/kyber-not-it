use std::{
    io::{self, Write},
    thread,
    time::Duration,
};

use rand::Rng;

use crate::{profiler, Bridge};

fn read_line() -> String {
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Failed to read line");
    input
}

pub(crate) fn select_command() {
    let mut stdout = io::stdout();

    println!("Welcome to KyberKracker!\n");
    println!("This is a tool for profiling and attacking DRAM with RowHammer, with the goal of cracking the Kyber cipher.\n");

    println!("Available commands:");
    println!("\t1. Profile");
    println!("\t2. Evaluate");
    println!("\t3. Attack");
    println!("\t4. Exit\n");

    print!("Select command (1-4): ");

    stdout.flush().unwrap();

    loop {
        let input = read_line();
        match input.trim() {
            "1" => {
                run_profiler();
                break;
            }
            "2" => {
                run_evaluation();
                break;
            }
            "3" => {
                let mut rng = rand::thread_rng();
                let mut progress = 0;
                while progress < 100 {
                    print!("\rRunning attack ({}%)", progress);
                    stdout.flush().unwrap();
                    let sleep = rng.gen_range(0..50);
                    thread::sleep(Duration::from_millis(sleep));
                    progress += rng.gen_range(0..=2);
                }
                println!("\rRunning attack (done!)");
                println!("\n*** CRACKED KYBER! MASTER THESIS COMPLETE! ***\n");
                break;
            }
            "4" => {
                println!("Bye!");
                break;
            }
            _ => {
                print!("Please enter a valid command (1-3): ");
                stdout.flush().unwrap();
            }
        }
    }
}

fn run_profiler() {
    let mut stdout = io::stdout();

    println!("\t1. Run with default settings (-p 0.5 -c 4 -d 2 -b haswell)");
    println!("\t2. Run with custom settings\n");

    print!("Select command (1-2): ");
    stdout.flush().unwrap();

    loop {
        let input = read_line();
        match input.trim() {
            "1" => {
                profiler::rowhammer::main(0.5, 4, 2, Bridge::Haswell);
                break;
            }
            "2" => {
                let fraction_of_phys_memory: f64 = loop {
                    print!("Fraction of physical memory to profile (0.0-1.0): ");
                    stdout.flush().unwrap();
                    let input = read_line();
                    if let Ok(f) = input.trim().parse() {
                        if f >= 0.0 && f <= 1.0 {
                            break f;
                        } else {
                            eprintln!("Value must be in the range 0.0-1.0");
                        }
                    } else {
                        eprintln!("Input must be a valid float number");
                    }
                };
                let cores: u8 = loop {
                    print!("Number of cores on target machine: ");
                    stdout.flush().unwrap();
                    let input = read_line();
                    if let Ok(c) = input.trim().parse() {
                        break c;
                    } else {
                        eprintln!("Input must be an integer");
                    }
                };
                let dimms: u8 = loop {
                    print!("Number of RAM sticks on target machine: ");
                    stdout.flush().unwrap();
                    let input = read_line();
                    if let Ok(d) = input.trim().parse() {
                        break d;
                    } else {
                        eprintln!("Input must be an integer");
                    }
                };
                let bridge = loop {
                    print!("Northbridge type (haswell/sandy): ");
                    stdout.flush().unwrap();
                    let input = read_line();
                    match input.trim() {
                        "haswell" => break Bridge::Haswell,
                        "sandy" => break Bridge::Sandy,
                        _ => eprintln!("Input must be either 'haswell' or 'sandy'"),
                    }
                };
                profiler::rowhammer::main(fraction_of_phys_memory, cores, dimms, bridge);
                break;
            }
            _ => {
                print!("Please enter a valid command (1-2): ");
                stdout.flush().unwrap();
            }
        }
    }
}

fn run_evaluation() {
    let mut stdout = io::stdout();

    println!("\t1. Run with default settings (-d 2)");
    println!("\t2. Run with custom settings\n");

    print!("Select command (1-2): ");
    stdout.flush().unwrap();

    loop {
        let input = read_line();
        match input.trim() {
            "1" => {
                profiler::pagefinder::main(2);
                break;
            }
            "2" => {
                let dimms: u8 = loop {
                    print!("Number of RAM sticks on target machine: ");
                    stdout.flush().unwrap();
                    let input = read_line();
                    if let Ok(d) = input.trim().parse() {
                        break d;
                    } else {
                        eprintln!("Input must be an integer");
                    }
                };
                profiler::pagefinder::main(dimms);
                break;
            }
            _ => {
                print!("Please enter a valid command (1-2): ");
                stdout.flush().unwrap();
            }
        }
    }
}
