#!/bin/bash

declare -a offsets=("25" "28" "32" "35" "39" "41" "46" "49")

cd src/degradation
gcc degrade.c -L ../libs -lmastik -ldwarf -lelf -lbfd -O3 -o degrade
cd ../..

echo "" > tempfile.out

for offset in "${offsets[@]}"; do
  echo "running with offset $offset"
  for i in {0,1,2,3}; do
    declare mask="0x$((1<<$i))0"
    taskset $mask ./src/degradation/degrade $offset &
    sleep 0.1
    echo -n "mask=$mask,offset=$offset: " >> tempfile.out
    taskset "0x$((1<<$i))" /home/development/Frodo/PQCrypto-LWEKE/frodo640/test_KEM >> tempfile.out
    pkill -f "degrade"
  done

done

cat tempfile.out | grep mask
rm tempfile.out
