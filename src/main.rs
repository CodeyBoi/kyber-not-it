mod profiler;
mod tui;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Profiles bit flip locations and suitable pages for a rowhammer attack
    Profile(ProfilerArgs),
    Evaluate(ProfilerArgs),
}

#[derive(Args, Debug)]
struct ProfilerArgs {
    /// How much of the physical memory that should be allocated during profiling
    #[arg(long, short = 'p', default_value_t = 0.5)]
    fraction_of_phys_memory: f64,
    /// How many cores are on the target machine
    #[arg(long, short, default_value_t = 4)]
    cores: u8,
    /// How many ram sticks on the target machine
    #[arg(long, short, default_value_t = 2)]
    dimms: u8,
    /// Which northbridge your CPU has (affects the DRAM mapping)
    #[arg(long, short, value_enum, default_value = "haswell")]
    bridge: Bridge,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Bridge {
    Haswell,
    Sandy,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        None => tui::select_command(),
        Some(command) => match command {
            Command::Profile(args) => {
                profiler::rowhammer::main(
                    args.fraction_of_phys_memory,
                    args.cores,
                    args.dimms,
                    args.bridge,
                );
            }
            Command::Evaluate(args) => {
                profiler::pagefinder::main(args.dimms);
            }
        },
    }
}
