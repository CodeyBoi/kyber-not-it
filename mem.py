import sys


def main():
    mem_used = 0
    for line in sys.stdin:
        print(line)
        start, end = line.replace("-", " ").split("-")[:2]
        mem_used += int(end, 16) - int(start, 16)

    print(f"Memory used: {mem_used} bytes")


if __name__ == "__main__":
    main()

# 7950336
# 15904768
