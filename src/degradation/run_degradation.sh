#! /bin/bash

# Function to compile 'degrade' if it doesn't exist
compile_degrade() {
    if [ ! -f degrade ]; then
        echo "degrade file does not exist. Compiling..."
        gcc degrade.c -L../libs -lmastik -ldwarf -lelf -lbfd -O3 -o degrade
    fi
}

# Function to run degradation with taskset masks
run_degradation(){
    # Declare an array of taskset masks
    declare -a cpu_masks=("0x2" "0x20" "0x4" "0x1")
    for mask in "${cpu_masks[@]}"; do
        # Run the degradation binary with the taskset mask
        taskset $mask ./degrade &
    done
}

# Function to kill all the degradation processes
kill_degradation()  {
    pkill -f "degrade"
}

# Compile the 'degrade' binary
compile_degrade

# Run degradations
run_degradation

# Run the FrodoKEM script
/home/development/Frodo/PQCrypto-LWEKE/frodo640/test_KEM &

# Wait for all the the FrodoKEM process to finish
wait

# Kill the degradation processes
kill_degradation