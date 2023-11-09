#! /bin/bash

# Check if the binary file exists
if [ ! -f degrade ]; then
    # Compile the source code if the binary file does not exist
    echo "degrade file does not exist. Compiling..."
    gcc degrade.c -L../libs -lmastik -ldwarf -lelf -lbfd -O3 -o degrade
fi

# Declare an array of taskset masks
declare -a cpu_masks=("0x2" "0x20" "0x4" "0x1")

run_degradation(){
    for mask in "${cpu_masks[@]}"; do
        # Run the degradation binary with the taskset mask
        taskset $mask ./degrade &
    done
}

kill_degradation()  {
    # Kill all the degradation processes
    pkill -f "degrade"
}

# Run degradations
run_degradation

# Run the FrodoKEM script
/home/development/Frodo/PQCrypto-LWEKE/frodo640/test_KEM &

# Wait for all the the FrodoKEM process to finish
wait

# Kill the degradation processes
kill_degradation