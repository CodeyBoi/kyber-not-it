import sys

start = ": pfn"
end = "soft-dirty"

pfns = set()
matches = []
for line in sys.stdin:
    pfn = line[line.find(start) + len(start) : line.find(end)].strip()
    if pfn == "0":
        continue
    print(pfn, end="")
    if pfn in pfns:
        print(" match!", end="")
        matches.append(pfn)
    pfns.add(pfn)
    print()

print(f"{len(matches)} matches")
