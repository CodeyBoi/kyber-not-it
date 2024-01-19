import sys

def check_bit_256(file_path):
    with open(file_path, 'r') as file:
        for line in file:
            line = line.strip()

            if "value=" not in line:
                continue

            value_start = line.find('value=') + len('value=')
            value = int(line[value_start:])

            if value & (1 << 8):
                print(f"Line: {line} - 8th bit is set for value={value}")
            #else:
            #    print(f"Line: {line} - 8th bit is not set for value={value}")

if __name__=='__main__':
    
    file_path = sys.argv[1]
    check_bit_256(file_path)
