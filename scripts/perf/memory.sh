#!/usr/bin/env bash
# Where a running pet's memory goes, on Linux (fn-1 task 29): its resident memory grouped by what is mapped (the GPU
# drivers and GL/Vulkan libraries, the pet's own heap and other anonymous memory, the binary, fonts, other
# libraries), RSS and PSS for each, then the anonymous memory in detail and the threads by name.
#
#   scripts/perf/memory.sh <pid>
#   scripts/perf/memory.sh $(pgrep -x AiPet)
#
# RSS counts a shared library in full in every process that maps it; PSS divides it among them, so PSS is what the
# pet costs and RSS what it shows in a task manager. Read the numbers once the pet has settled (a minute or so).
set -euo pipefail

pid=${1:?usage: $0 <pid>}
[[ -r /proc/$pid/smaps ]] || { echo "no process $pid (or it isn't yours)" >&2; exit 1; }

python3 - "$pid" <<'EOF'
import collections, re, sys

pid = sys.argv[1]

def group(name):
    n = name.lower()
    if not name or name == "[heap]" or name.startswith("[anon"):
        return "anonymous: heap, arenas, buffers"
    if name.startswith("[stack"):
        return "main stack"
    if name.startswith("["):
        return "kernel mappings " + name
    if n.startswith("/dev/dri") or "/dri/" in n:
        return "GPU device memory"
    gpu = ("nvidia", "libgallium", "libllvm", "libvulkan", "libegl", "libgl", "libgbm", "libdrm", "mesa", "_dri.so")
    if any(k in n for k in gpu):
        return "GPU drivers and GL/Vulkan libraries"
    if n.endswith("/aipet") or n.endswith("/aipet.exe"):
        return "the AiPet binary"
    if "/fonts/" in n or n.endswith((".ttf", ".otf", ".ttc", ".woff2")):
        return "fonts"
    if ".so" in n:
        return "other shared libraries"
    if "memfd" in n or "/dev/shm" in n:
        return "shared memory (Wayland buffers)"
    return "other files"

groups = collections.defaultdict(lambda: [0, 0])
anon = []
cur = None
for line in open(f"/proc/{pid}/smaps"):
    m = re.match(r"^([0-9a-f]+)-([0-9a-f]+) (\S+) \S+ \S+ \S+\s*(.*)$", line)
    if m:
        name = m.group(4).strip()
        cur = {"group": group(name), "name": name, "size": (int(m.group(2), 16) - int(m.group(1), 16)) // 1024,
               "rss": 0, "pss": 0}
        if cur["group"].startswith("anonymous"):
            anon.append(cur)
        continue
    if line.startswith("Rss:"):
        cur["rss"] = int(line.split()[1])
        groups[cur["group"]][0] += cur["rss"]
    elif line.startswith("Pss:"):
        cur["pss"] = int(line.split()[1])
        groups[cur["group"]][1] += cur["pss"]

total = [sum(v[0] for v in groups.values()), sum(v[1] for v in groups.values())]
print(f"{'':40} {'RSS MB':>8} {'PSS MB':>8}")
for g, (r, p) in sorted(groups.items(), key=lambda x: -x[1][0]):
    if r >= 512:
        print(f"{g:40} {r / 1024:8.1f} {p / 1024:8.1f}")
print(f"{'total':40} {total[0] / 1024:8.1f} {total[1] / 1024:8.1f}")

heap = sum(m["rss"] for m in anon if m["name"] == "[heap]")
big = [m for m in anon if m["name"] != "[heap]" and m["rss"] >= 4096]
small = [m for m in anon if m["name"] != "[heap]" and m["rss"] < 4096]
print(f"\nanonymous: the main heap {heap / 1024:.1f} MB; {len(big)} blocks of 4 MB or more "
      f"{sum(m['rss'] for m in big) / 1024:.1f} MB; {len(small)} smaller blocks "
      f"{sum(m['rss'] for m in small) / 1024:.1f} MB (thread arenas and stacks, buffers)")
for m in sorted(big, key=lambda m: -m["rss"])[:8]:
    print(f"  {m['rss'] / 1024:6.1f} MB resident of a {m['size'] / 1024:.1f} MB block")

threads = collections.Counter()
import os
for t in os.listdir(f"/proc/{pid}/task"):
    try:
        threads[open(f"/proc/{pid}/task/{t}/comm").read().strip()] += 1
    except OSError:
        pass
print(f"\nthreads: {sum(threads.values())}: " + ", ".join(f"{n} ×{c}" if c > 1 else n for n, c in threads.most_common()))
EOF
