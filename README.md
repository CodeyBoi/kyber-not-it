# Bit's Not It
*The program setting the bit that's not it.*

## Usage

The program has three subcommands; `profile`, `evaluate` and `attack`. These can be run by
```bash
path/to/binary <subcommand> [options]
```
*NOTE: All subcommands need to be run as root to function correctly.*

### Profile
The `profile` subcommand is used to profile the system to find pages which are particularly vulnerable to RowHammer flips. It will output a file containing the profiled pages with data of how many flips were found on each page.

 It takes the following options:
- `-p`: The fraction of the physical memory on the target machine to be profiled. Defaults to 0.5.
- `-c, --cores`: The amount of cores on the target machine. Defaults to 4.
- `-d, --dimms`: The amount of RAM sticks on the target machine. Defaults to 2.
- `-b --bridge`: Which northbridge the CPU on the target machine uses. Defaults to `haswell`.
- `-o, --output`: The file to write the profile to. Defaults to `flips.out`.
- `-a, --attack-method`: The attack method to use. Defaults to `rowhammer`. (`rowpress` seems to not work on DDR3 systems)

### Evaluate
The `evaluate` subcommand is used to evaluate the profiled pages to find the best pages to flip. This is a deeper test which specifically tests the pages found to be potentially vulnerable by the `profile` subcommand. It will output a file containing the evaluated pages with data of how many flips were found on each page. It will output a file for each page containing the bitindices of the bits which are highly vulnerable to RowHammer flips, meaning they flipped every time they were targeted.

It takes the following options:
- `-p`: The fraction of the physical memory on the target machine to be profiled. Defaults to 0.5.

### Attack
The `attack` subcommand is used to attack the target process (FrodoKEM). It will allocate memory until it finds at least three pages which are highly vulnerable to RowHammer flips. It will then flip the bits on these pages to change the value of the error matrix. It will then check if the key has changed and if it has, it will print the new key.

It takes the following options:
- `-p`: The fraction of the physical memory on the target machine to be profiled. Defaults to 0.5.
- `-t --testing`: If set, the program will not actually run the attack, but will instead evaluate how much time is needed to flip the required bits. Defaults to false.