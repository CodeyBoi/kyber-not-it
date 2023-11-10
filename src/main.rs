mod attack;
mod attack_tester;
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
    Attack(AttackArgs),
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
    #[arg(long, short, value_enum, default_value_t)]
    bridge: Bridge,
    /// File used to save the output
    #[arg(long, short, default_value = "flips.out")]
    output: String,
    #[arg(long, short, value_enum, default_value_t)]
    attack_method: AttackMethod,
}

impl Default for ProfilerArgs {
    fn default() -> Self {
        Self {
            fraction_of_phys_memory: 0.5,
            cores: 4,
            dimms: 2,
            bridge: Bridge::Haswell,
            output: "flips.out".to_string(),
            attack_method: AttackMethod::RowHammer,
        }
    }
}

#[derive(Args, Debug)]
struct AttackArgs {
    #[arg(long, short = 'p', default_value_t = 0.5)]
    fraction_of_phys_memory: f64,
    #[arg(long, short, action)]
    testing: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Bridge {
    Haswell,
    Sandy,
}

impl Default for Bridge {
    fn default() -> Self {
        Bridge::Haswell
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AttackMethod {
    RowHammer,
    RowPress,
}

impl Default for AttackMethod {
    fn default() -> Self {
        AttackMethod::RowHammer
    }
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
                    args.output,
                    args.attack_method,
                );
            }
            Command::Evaluate(args) => {
                profiler::pagefinder::main(args.dimms);
            }
            Command::Attack(args) => {
                //attack::attack::main(args.fraction_of_phys_memory, args.testing);
                attack::degrade::main();
            }
        },
    }
}
