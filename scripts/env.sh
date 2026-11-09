#!/usr/bin/env bash
# Record the machine and the exact versions: results/env.json
HERE=$(cd "$(dirname "$0")/.." && pwd)
cd "$HERE/harness"
lock() { python3 -c "import re,sys; s=open('Cargo.lock').read(); m=re.search(r'name = \"$1\"\nversion = \"[^\"]+\"\nsource = \"git\+[^#]+#([0-9a-f]+)\"', s); print(m.group(1) if m else '?')"; }
python3 - "$(lock controlledburn)" "$(lock burnscars)" "$(rustc --version)" "$(git -C "$HERE" rev-parse --short HEAD 2>/dev/null || echo none)" <<'PY' > "$HERE/results/env.json"
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
}, indent=1))
PY
cat "$HERE/results/env.json"
