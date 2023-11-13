use std::io::{self, Write};

use crate::{attack, attack_tester, profiler, AttackArgs, AttackMethod, Bridge, ProfilerArgs};

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
                run_attack();
                //let mut rng = rand::thread_rng();
                //let mut progress = 0;
                //while progress < 100 {
                //    print!("\rRunning attack ({}%)", progress);
                //    stdout.flush().unwrap();
                //    let sleep = rng.gen_range(0..50);
                //    thread::sleep(Duration::from_millis(sleep));
                //    progress += rng.gen_range(0..=2);
                //}
                //println!("\rRunning attack (done!)");
                println!("\n*** CRACKED KYBER! MASTER THESIS COMPLETE! ***\n");
                break;
            }
            "4" => {
                attack_tester::tester::main();
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

    let mut opts = ProfilerArgs::default();
    println!(
        "\t1. Run with default settings (-p {} -c {} -d {} -b {:?} -o {} -a {:?})",
        opts.fraction_of_phys_memory,
        opts.cores,
        opts.dimms,
        opts.bridge,
        opts.output,
        opts.attack_method
    );
    println!("\t2. Run with custom settings\n");

    print!("Select command (1-2): ");
    stdout.flush().unwrap();

    loop {
        let input = read_line();
        match input.trim() {
            "1" => {
                profiler::rowhammer::main(
                    opts.fraction_of_phys_memory,
                    opts.cores,
                    opts.dimms,
                    opts.bridge,
                    opts.output,
                    opts.attack_method,
                );
                break;
            }
            "2" => {
                opts.fraction_of_phys_memory = loop {
                    print!(
                        "Fraction of physical memory to profile ({:.1}): ",
                        opts.fraction_of_phys_memory
                    );
                    stdout.flush().unwrap();
                    let input = read_line();
                    if input.trim().is_empty() {
                        break opts.fraction_of_phys_memory;
                    }
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
                opts.cores = loop {
                    print!("Number of cores on target machine ({}): ", opts.cores);
                    stdout.flush().unwrap();
                    let input = read_line();
                    if input.trim().is_empty() {
                        break opts.cores;
                    }
                    if let Ok(c) = input.trim().parse() {
                        break c;
                    } else {
                        eprintln!("Input must be an integer");
                    }
                };
                opts.dimms = loop {
                    print!("Number of RAM sticks on target machine ({}): ", opts.dimms);
                    stdout.flush().unwrap();
                    let input = read_line();
                    if input.trim().is_empty() {
                        break opts.dimms;
                    }
                    if let Ok(d) = input.trim().parse() {
                        break d;
                    } else {
                        eprintln!("Input must be an integer");
                    }
                };
                opts.bridge = loop {
                    print!("Northbridge type ({:?}): ", opts.bridge);
                    stdout.flush().unwrap();
                    let input = read_line();
                    if input.trim().is_empty() {
                        break opts.bridge;
                    }
                    match input.trim().to_lowercase().as_str() {
                        "haswell" => break Bridge::Haswell,
                        "sandy" => break Bridge::Sandy,
                        _ => eprintln!("Input must be either 'haswell' or 'sandy'"),
                    }
                };
                opts.output = loop {
                    print!("Output file ({}): ", opts.output);
                    stdout.flush().unwrap();
                    let input = read_line();
                    if input.trim().is_empty() {
                        break opts.output;
                    } else {
                        break input.trim().to_string();
                    }
                };
                opts.attack_method = loop {
                    print!("Attack method ({:?}): ", opts.attack_method);
                    stdout.flush().unwrap();
                    let input = read_line();
                    if input.trim().is_empty() {
                        break opts.attack_method;
                    }
                    match input.trim().to_lowercase().as_str() {
                        "rowhammer" => break AttackMethod::RowHammer,
                        "rowpress" => break AttackMethod::RowPress,
                        _ => eprintln!("Input must be either 'rowhammer' or 'rowpress'"),
                    }
                };
                println!(
                    "Selected settings: -p {} -c {} -d {} -b {:?} -o {} -a {:?}",
                    opts.fraction_of_phys_memory,
                    opts.cores,
                    opts.dimms,
                    opts.bridge,
                    opts.output,
                    opts.attack_method
                );
                profiler::rowhammer::main(
                    opts.fraction_of_phys_memory,
                    opts.cores,
                    opts.dimms,
                    opts.bridge,
                    opts.output,
                    opts.attack_method,
                );
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

    let mut opts = ProfilerArgs::default();
    println!("\t1. Run with default settings (-d {})", opts.dimms);
    println!("\t2. Run with custom settings\n");

    print!("Select command (1-2): ");
    stdout.flush().unwrap();

    loop {
        let input = read_line();
        match input.trim() {
            "1" => {
                profiler::pagefinder::main(opts.dimms);
                break;
            }
            "2" => {
                opts.dimms = loop {
                    print!("Number of RAM sticks on target machine: ");
                    stdout.flush().unwrap();
                    let input = read_line();
                    if input.trim().is_empty() {
                        break opts.dimms;
                    }
                    if let Ok(d) = input.trim().parse() {
                        break d;
                    } else {
                        eprintln!("Input must be an integer");
                    }
                };
                println!("Selected settings: -d {}", opts.dimms);
                profiler::pagefinder::main(opts.dimms);
                break;
            }
            _ => {
                print!("Please enter a valid command (1-2): ");
                stdout.flush().unwrap();
            }
        }
    }
}

fn run_attack() {
    let mut stdout = io::stdout();

    let mut opts = AttackArgs::default();

    println!(
        "\t1. Run with default settings (-p {}, {})",
        opts.fraction_of_phys_memory, opts.testing
    );
    println!("\t2. Run with custom settings\n");

    print!("Select command (1-2): ");
    stdout.flush().unwrap();

    loop {
        let input = read_line();
        match input.trim() {
            "1" => {
                attack::attack::main(opts.fraction_of_phys_memory, opts.dimms, opts.testing);
                break;
            }
            "2" => {
                opts.fraction_of_phys_memory = loop {
                    print!("Fraction of physical memory to profile (0.0-1.0): ");
                    stdout.flush().unwrap();
                    let input = read_line();
                    if input.trim().is_empty() {
                        break opts.fraction_of_phys_memory;
                    }
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

                opts.dimms = loop {
                    print!("Number of RAM sticks on target machine: ");
                    stdout.flush().unwrap();
                    let input = read_line();
                    if input.trim().is_empty() {
                        break opts.dimms;
                    }
                    if let Ok(d) = input.trim().parse() {
                        break d;
                    } else {
                        eprintln!("Input must be an integer");
                    }
                };

                opts.testing = loop {
                    print!("Testing mode (true/false, t/f): ");
                    stdout.flush().unwrap();
                    let input = read_line();
                    if input.trim().is_empty() {
                        break opts.testing;
                    }
                    match input.trim() {
                        "true" | "t" => break true,
                        "false" | "f" => break false,
                        _ => eprintln!("Input must be either 'true, t' or 'false, f'"),
                    }
                };

                attack::attack::main(opts.fraction_of_phys_memory, opts.dimms, opts.testing);
                break;
            }
            _ => {
                print!("Please enter a valid command (1-2): ");
                stdout.flush().unwrap();
            }
        }
    }
}
