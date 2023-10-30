use std::{
    fs::File,
    io::{self, BufRead, BufReader, Write},
    process::{self, Command},
    thread,
    time::Duration,
};

use nix::{
    sched::{sched_setaffinity, CpuSet},
    unistd::{fork, ForkResult, Pid},
};

fn run_testing() {
    let file = File::open("offset.txt").unwrap();
    let reader = BufReader::new(file);
    let lines = reader.lines();

    for offset in lines {
        let offset = offset.unwrap();
        io::stdout().flush().unwrap();

        println!("Testing offset: {}", offset);
        match unsafe { fork() } {
            Ok(ForkResult::Parent { child }) => {
                thread::sleep(Duration::from_secs(1));

                let mut cpu_set = CpuSet::new();
                cpu_set.set(5).unwrap();
                sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();

                let ok = Command::new("../FrodoFLIP/FrodoKEM-Rowhammer/frodokem/Reference_Implementation/reference/FrodoKEM-640/frodo/test_KEM")
                    .status()
                    .expect("running test_KEM failed.");

                let ok = Command::new("sudo")
                    .arg("pkill")
                    .arg("-f")
                    .arg("degrade")
                    .status()
                    .expect("Failed to kill degrade process");
            }

            Ok(ForkResult::Child) => {
                let mut cpu_set = CpuSet::new();
                cpu_set.set(1).unwrap();
                sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();

                let ok =
                    Command::new("../FrodoFLIP/FrodoKEM-Rowhammer/frodokem-sanitycheck/degrade")
                        .arg(offset)
                        .status()
                        .expect("Running degrade failed");

                process::exit(0);
            }

            Err(_) => {
                println!("Failed to fork process");
                process::exit(1);
            }
        }
    }
}

pub(crate) fn main() {
    run_testing();
}
