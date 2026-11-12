#!/usr/bin/env bash
# Record the machine and the exact versions: $CBCMP_RESULTS/env.json
HERE=$(cd "$(dirname "$0")/.." && pwd)
RES=${CBCMP_RESULTS:-$HERE/results}; mkdir -p "$RES"
COMMIT=${CBCMP_COMMIT:-$(git -C "$HERE" rev-parse --short HEAD 2>/dev/null || echo none)}
cd "$HERE/harness"
# A git dependency's commit, or a registry dependency's version.
lock() { python3 -c "import re,sys; s=open('Cargo.lock').read(); m=re.search(r'name = \"$1\"\nversion = \"([^\"]+)\"\nsource = \"([^\"]+)\"', s); print('?' if not m else (m.group(2).split('#')[1] if m.group(2).startswith('git+') else m.group(1)))"; }
python3 - "$(lock controlledburn)" "$(lock burnscars)" "$(rustc --version 2>/dev/null || cat /opt/rustc-version 2>/dev/null || echo unknown)" "$COMMIT" <<'PY' > "$RES/env.json"
import json, os, platform, subprocess, sys
def sh(c):
    try: return subprocess.run(c, shell=True, capture_output=True, text=True).stdout.strip()
    except Exception: return ""
lscpu = dict(l.split(":", 1) for l in sh("lscpu").splitlines() if ":" in l)
print(json.dumps({
  "controlledburn_rs": sys.argv[1], "burnscars": sys.argv[2], "rustc": sys.argv[3], "harness": sys.argv[4],
  "cpu": lscpu.get("Model name", "").strip(), "sockets": lscpu.get("Socket(s)", "").strip(),
  "cores_per_socket": lscpu.get("Core(s) per socket", "").strip(), "threads_per_core": lscpu.get("Thread(s) per core", "").strip(),
  "nproc": os.cpu_count(), "mem_gb": round(os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_PHYS_PAGES") / 2**30, 1),
  "os": sh("grep PRETTY_NAME /etc/os-release | cut -d= -f2 | tr -d '\"'"), "kernel": platform.release(),
  "hostname_class": os.environ.get("CBCMP_MACHINE", "unnamed"),
  "slurm_job": os.environ.get("SLURM_JOB_ID", ""),
  "container_image": os.environ.get("CBCMP_IMAGE", ""),
  # what decides first-touch page-fault costs (design/slice-after-burn.md in controlledburn-rs)
  "thp_enabled": sh("cat /sys/kernel/mm/transparent_hugepage/enabled"),
  "thp_defrag": sh("cat /sys/kernel/mm/transparent_hugepage/defrag"),
  "glibc": sh("getconf GNU_LIBC_VERSION") or sh("ldd --version | head -1"),
  "glibc_tunables": os.environ.get("GLIBC_TUNABLES", ""),
  "malloc_arena_max": os.environ.get("MALLOC_ARENA_MAX", ""),
}, indent=1))
PY
cat "$RES/env.json"
