#!/usr/bin/env bash
# Regenerate every number and figure of the comparison, from source.
#   ./run_all.sh            full plan (meant for a dedicated node, see pawsey/)
#   ./run_all.sh --quick    small inputs, a smoke test (minutes)
# Env: PLAN_ARGS (extra args for scripts/plan.py), CBCMP_MACHINE (a label).
# Needs the Rust toolchain in harness/rust-toolchain.toml, python3 with GDAL
# bindings (to prepare the rock outcrop input) and matplotlib. Or use the
# container, which has all of it built in: see README.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); cd "$HERE"
[ "${1:-}" = "--quick" ] && export PLAN_ARGS="--quick ${PLAN_ARGS:-}"
echo "== build" >&2
(cd harness && cargo build --release --locked)
echo "== inputs" >&2
mkdir -p inputs
[ -s inputs/small-1e5.wkbs ] || harness/target/release/synth inputs
[ -s inputs/rock.wkbs ] || python3 -I scripts/prepare_rock.py data inputs
scripts/job.sh
