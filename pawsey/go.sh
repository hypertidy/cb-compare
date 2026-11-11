#!/bin/bash
# Run cb-compare on Setonix by hand, from a git checkout on a login node.
#
#   git clone https://github.com/hypertidy/cb-compare   # or: git pull
#   cd cb-compare
#   pawsey/go.sh check    # pull the image for this commit and run one check (minutes, login node)
#   pawsey/go.sh quick    # submit the smoke test (~30 min, 1 node)
#   pawsey/go.sh full     # submit the full plan (up to 12 h, 1 exclusive node)
#   pawsey/go.sh status   # what has finished so far
#   pawsey/go.sh pack     # tar the results to bring back
#
# Big files (image, cache, results) go under $MYSCRATCH, not $HOME.
# Results go in a directory per image tag, $WORK/results-<tag> (quick:
# results-quick-<tag>). A resubmit with the same tag resumes it; a new
# commit starts afresh instead of skipping configurations an older image timed.
# Env: ACCOUNT (default $PAWSEY_PROJECT), WORK (default $MYSCRATCH/cb-compare),
#      TAG (default sha-<this commit>; check and quick fall back to latest,
#      full doesn't), RESULTS (override the results directory).
set -euo pipefail
cd "$(dirname "$0")/.."
REPO=$PWD
WORK=${WORK:-${MYSCRATCH:-$PWD}/cb-compare}
ACCOUNT=${ACCOUNT:-${PAWSEY_PROJECT:-}}
mkdir -p "$WORK"
export SINGULARITY_CACHEDIR=${SINGULARITY_CACHEDIR:-$WORK/.cache}
export APPTAINER_CACHEDIR=$SINGULARITY_CACHEDIR
export SINGULARITY_TMPDIR=${SINGULARITY_TMPDIR:-$WORK/.tmp}
export APPTAINER_TMPDIR=$SINGULARITY_TMPDIR
mkdir -p "$SINGULARITY_CACHEDIR" "$SINGULARITY_TMPDIR"

if ! command -v singularity >/dev/null; then
  module load singularity/4.1.0-slurm 2>/dev/null || module load singularity 2>/dev/null || {
    echo "no singularity: try 'module avail singularity' and load it first" >&2; exit 1; }
fi

TAG=${TAG:-sha-$(git rev-parse --short=7 HEAD)}
SIF=$WORK/cb-compare-$TAG.sif
pull() {  # pull [exact]: with exact, no fallback to latest
  [ -s "$SIF" ] && return
  echo "pulling ghcr.io/hypertidy/cb-compare:$TAG -> $SIF" >&2
  if ! singularity pull "$SIF" "docker://ghcr.io/hypertidy/cb-compare:$TAG"; then
    if [ "${1:-}" = exact ]; then
      echo "no image for $TAG yet (CI builds one per push to main, ~3 min); try again shortly" >&2
      exit 1
    fi
    echo "no image for $TAG (CI builds one per push to main, ~3 min); using latest" >&2
    TAG=latest; SIF=$WORK/cb-compare-latest.sif
    [ -s "$SIF" ] || singularity pull "$SIF" docker://ghcr.io/hypertidy/cb-compare:latest
  fi
}
need_account() {
  [ -n "$ACCOUNT" ] || { echo "set ACCOUNT=<project> (or PAWSEY_PROJECT)" >&2; exit 1; }
}
submit() {  # submit <results dir> [sbatch args...]
  local res=$1; shift
  mkdir -p "$res"
  # exported, not listed in --export, so values with commas survive
  (cd "$WORK" && SIF=$SIF RESULTS=$res PLAN_ARGS=${PLAN_ARGS:-} \
     sbatch --account="$ACCOUNT" --export=ALL "$@" "$REPO/pawsey/setonix.slurm")
  echo "logs: $WORK/slurm-*.log   results: $res" >&2
}

case ${1:-} in
  check)
    pull
    singularity exec "$SIF" /opt/cb-compare/harness/target/release/verify \
      rects /opt/cb-compare/inputs coverage | head -c 400; echo
    echo "image OK: $SIF" >&2 ;;
  quick)
    need_account; pull
    PLAN_ARGS=--quick submit "${RESULTS:-$WORK/results-quick-$TAG}" --time=00:30:00 ;;
  full)
    need_account; pull exact
    submit "${RESULTS:-$WORK/results-$TAG}" "${@:2}" ;;
  status)
    for d in "$WORK"/results*; do
      [ -d "$d" ] || continue
      p=$(ls "$d"/plan*.tsv 2>/dev/null | head -1 || true)
      echo "$d: $(ls "$d"/timing/*.jsonl 2>/dev/null | wc -l) of $( [ -n "$p" ] && wc -l < "$p" || echo '?') timed," \
           "$(ls "$d"/verify/*.json 2>/dev/null | wc -l) verified, summary: $([ -s "$d/summary.md" ] && echo yes || echo no)"
    done
    squeue --me 2>/dev/null || true ;;
  pack)
    out=$WORK/cb-compare-results-$(date +%Y%m%d-%H%M).tar.gz
    tar -C "$WORK" -czf "$out" $(cd "$WORK" && ls -d results* slurm-*.log 2>/dev/null)
    echo "$out" ;;
  *) sed -n '2,18p' "$0"; exit 1 ;;
esac
