#! /bin/bash

# Check if the binary file exists
if [ ! -f degrade ]; then
    # Compile the source code if the binary file does not exist
    echo "degrade file does not exist. Compiling..."
    gcc degrade.c -L../libs -lmastik -ldwarf -lelf -lbfd -O3 -o degrade
fi

