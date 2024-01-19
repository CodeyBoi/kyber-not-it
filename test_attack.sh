#! /bin/bash

#cargo build --release

for i in {32..500} #{35..60}
do
    echo "Running with $i"

    sudo ./target/release/kyber-not-it attack -p 0.95 -n $i > ./attack_results/out_$i.txt 2>&1

    python3 check_flips.py ./attack_results/out_$i.txt > ./number_flips.txt

    number_flips=$(wc -l < ./number_flips.txt)

    if [ $number_flips -gt "0" ]; then
        echo "$i pages needed for flip, found $number_flips" >> attack_results.txt
    else
        echo "Found no flips at $i, got $number_flips" >> attack_results.txt
    fi

    sudo rm number_flips.txt
done

for i in {501..1000} #{35..60}
do
    echo "Running with $i"

    sudo ./target/release/kyber-not-it attack -p 0.95 -n $i > ./attack_results/out_$i.txt 2>&1

    python3 check_flips.py ./attack_results/out_$i.txt > ./number_flips.txt

    number_flips=$(wc -l < ./number_flips.txt)

    if [ $number_flips -gt "0" ]; then
        echo "$i pages needed for flip, found $number_flips" >> attack_results.txt
    else
        echo "Found no flips at $i, got $number_flips" >> attack_results.txt
    fi

    sudo rm number_flips.txt
done
#for i in {633..1199}
#do
#    echo "Running with $i"

#     sudo ./target/release/kyber-not-it attack -p 0.95 -n $i > ./attack_results/out_$i.txt 2>&1

#    python3 check_flips.py ./attack_results/out_$i.txt > ./number_flips.txt

#    number_flips=$(wc -l < ./number_flips.txt)

#    if [ $number_flips -gt "0" ]; then
#        echo "$i pages needed for flip, found $number_flips" >> attack_results.txt
#    else
#        echo "Found no flips at $i, got $number_flips" >> attack_results.txt
#    fi

#    sudo rm number_flips.txt
#done
