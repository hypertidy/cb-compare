# The whole comparison in one image: the harness built at this commit and
# the prepared inputs. Nothing is built or downloaded at run time, so it
# runs read-only (Singularity/Apptainer on HPC) with one writable results
# directory bound in.
#
#   docker build -t cb-compare --build-arg CBCMP_COMMIT=$(git rev-parse --short HEAD) .
#   docker run --rm --read-only --tmpfs /tmp -v "$PWD/results":/results cb-compare
#   docker run --rm --read-only --tmpfs /tmp -e PLAN_ARGS=--quick -v "$PWD/results":/results cb-compare
#   singularity exec --bind "$PWD/results":/results cb-compare.sif /opt/cb-compare/scripts/job.sh

# ---- build: toolchain, harness, inputs (GDAL only to read the rock outcrop GPKG)
FROM ghcr.io/osgeo/gdal:ubuntu-small-3.11.0 AS build
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates build-essential \
    && rm -rf /var/lib/apt/lists/*
ARG RUST_VERSION=1.99.1
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain ${RUST_VERSION} --no-modify-path
ENV PATH=/opt/cargo/bin:$PATH
WORKDIR /opt/cb-compare
COPY harness harness
RUN cd harness && cargo build --release --locked && rustc --version > /opt/rustc-version
COPY data data
COPY scripts scripts
RUN mkdir inputs && harness/target/release/synth inputs \
    && python3 -I scripts/prepare_rock.py data inputs

# ---- run: binaries, inputs, scripts, python3 + matplotlib
FROM ubuntu:24.04
RUN apt-get update && apt-get install -y --no-install-recommends python3 python3-matplotlib time \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /opt/cb-compare
COPY --from=build /opt/cb-compare/harness/target/release/synth \
                  /opt/cb-compare/harness/target/release/verify \
                  /opt/cb-compare/harness/target/release/timing harness/target/release/
COPY --from=build /opt/cb-compare/harness/Cargo.toml /opt/cb-compare/harness/Cargo.lock \
                  /opt/cb-compare/harness/rust-toolchain.toml harness/
COPY --from=build /opt/cb-compare/inputs inputs
COPY --from=build /opt/rustc-version /opt/rustc-version
COPY data/SHA256SUMS data/rema32.grid.json data/
COPY scripts scripts
COPY run_all.sh README.md ./
ARG CBCMP_COMMIT=unknown
ENV CBCMP_COMMIT=${CBCMP_COMMIT} \
    CBCMP_INPUTS=/opt/cb-compare/inputs \
    CBCMP_BIN=/opt/cb-compare/harness/target/release \
    CBCMP_RESULTS=/results \
    MPLCONFIGDIR=/tmp
CMD ["/opt/cb-compare/scripts/job.sh"]
