# Everything run_all.sh needs: the pinned Rust toolchain, GDAL's Python
# bindings (input preparation only) and matplotlib (figures).
#   docker build -t cb-compare .
#   docker run --rm -v "$PWD":/w -w /w cb-compare ./run_all.sh --quick
# On HPC, convert it: singularity build cb-compare.sif docker-daemon://cb-compare:latest
FROM ghcr.io/osgeo/gdal:ubuntu-small-3.11.0
RUN apt-get update && apt-get install -y --no-install-recommends \
      curl ca-certificates build-essential git python3-matplotlib time \
    && rm -rf /var/lib/apt/lists/*
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.99.1 \
    && chmod -R a+rwX /opt/cargo /opt/rustup
