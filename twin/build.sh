#!/usr/bin/env bash
# builds the coverage twin: the stellar-core commit pinned in pins.toml, compiled
# with clang and --enable-codecoverage, packaged as shapekit/core-twin:<version>-cov.
#
# usage: twin/build.sh [--jobs N] [--memory SIZE] [--cpus N] [--workdir DIR]
#
#   --jobs     make parallelism (default 3); cargo gets the same number of jobs
#   --memory   memory limit of the compile container (default 6g)
#   --cpus     cpu limit of the compile container (default 4)
#   --workdir  where the source tree, cargo cache and build tree live (default
#              $TMPDIR/twin-build). it holds tens of gigabytes, so it must be on
#              disk; a path under /tmp is refused.
#
# a rerun reuses the work directory: the source is not fetched again and make
# continues where it stopped. the last lines printed are the measurement table:
# wall time per phase, peak memory of the compile container, disk used.
set -euo pipefail

jobs=3 memory=6g cpus=4 workdir=""
while [ $# -gt 0 ]; do
    case $1 in
        --jobs) jobs=$2; shift 2 ;;
        --memory) memory=$2; shift 2 ;;
        --cpus) cpus=$2; shift 2 ;;
        --workdir) workdir=$2; shift 2 ;;
        *) echo "unknown argument $1" >&2; exit 2 ;;
    esac
done

repo=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
twin="$repo/twin"
workdir=${workdir:-${TMPDIR:?set TMPDIR or pass --workdir}/twin-build}
mkdir -p "$workdir"
workdir=$(cd "$workdir" && pwd -P)
case $workdir in
    /tmp|/tmp/*) echo "refusing to build under /tmp ($workdir): it is memory-backed on most hosts" >&2; exit 2 ;;
esac

commit=$("$repo/scripts/pin.sh" git stellar-core)
tag=$("$repo/scripts/pin.sh" tag stellar-core)
version=${tag#v}
base=$("$repo/scripts/pin.sh" image twin-ubuntu)
url=$(python3 -I -c 'import sys,tomllib; print(next(e["url"] for e in tomllib.load(open(sys.argv[1],"rb"))["git"] if e["name"]=="stellar-core"))' "$repo/pins.toml")
image="shapekit/core-twin:${version}-cov"
toolchain="shapekit/core-twin-toolchain:${version}"

# core's release profile builds its rust library (all ten contract hosts in one
# crate graph) with fat lto, one codegen unit and full debug info. that single
# rustc invocation is killed at a 6 GB limit even without debug info, so the
# twin builds it without lto, in 16 codegen units and without debug info. these
# settings change how the rust code is optimized, not what it computes; the
# byte equality check (shapekit twin verify) is what shows the twin writes the
# same ledgers as the release build.
#
# flags of core's own docker/Dockerfile.testing build stage, minus -g1 (debug
# info is not needed for coverage and costs memory) and minus tracy and the
# next protocol, which the release build does not enable either
cc=clang-20 cxx=clang++-20
cflags='-O3 -fno-omit-frame-pointer'
cxxflags='-O3 -fno-omit-frame-pointer -stdlib=libc++'
configure_flags='--enable-codecoverage'

now() { date +%s; }
declare -A took

# source at the pinned commit, with submodules, on disk
t0=$(now)
src="$workdir/src"
if [ ! -e "$src/.twin-fetched" ]; then
    rm -rf "$src"
    git init -q "$src"
    git -C "$src" remote add origin "$url"
    git -C "$src" fetch -q --depth 1 origin "$commit"
    git -C "$src" checkout -q --detach FETCH_HEAD
    git -C "$src" submodule update -q --init --recursive --depth 1
    touch "$src/.twin-fetched"
fi
got=$(git -C "$src" rev-parse HEAD)
if [ "$got" != "$commit" ]; then
    echo "source in $src is at $got, pins.toml pins $commit; delete $workdir to refetch" >&2
    exit 1
fi
took[fetch]=$(( $(now) - t0 ))

# toolchain image
t0=$(now)
ctx="$workdir/toolchain-context"
rm -rf "$ctx" && mkdir -p "$ctx"
cp "$src/install-rust.sh" "$src/rust-toolchain.toml" "$ctx/"
docker build -q --target toolchain --build-arg "BASE=$base" -t "$toolchain" -f "$twin/Dockerfile" "$ctx" >/dev/null
took[toolchain]=$(( $(now) - t0 ))

# compile, inside memory and cpu limits, as the invoking user so the tree stays ours
t0=$(now)
mkdir -p "$workdir/cargo-home"
name="shapekit-twin-build-$$"
cleanup() { docker rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT
docker run -d --name "$name" --memory "$memory" --memory-swap "$memory" --cpus "$cpus" \
    --user "$(id -u):$(id -g)" -e HOME=/work -e CARGO_HOME=/work/cargo-home \
    -e CARGO_BUILD_JOBS="$jobs" -e CARGO_INCREMENTAL=0 -e CARGO_PROFILE_RELEASE_DEBUG=0 \
    -e CARGO_PROFILE_RELEASE_LTO=off -e CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
    -e CC="$cc" -e CXX="$cxx" -e CFLAGS="$cflags" -e CXXFLAGS="$cxxflags" \
    -e CONFIGURE_FLAGS="$configure_flags" -e JOBS="$jobs" \
    -v "$workdir:/work" -w /work/src "$toolchain" bash -c '
        set -e
        trap "cat /sys/fs/cgroup/memory.peak > /work/memory.peak" EXIT
        # cargo reads the toolchain through rustup, whose home is in the image
        export PATH=/work/cargo-home/bin:$PATH
        if [ ! -f Makefile ]; then
            ./autogen.sh
            ./configure CC="$CC" CXX="$CXX" CFLAGS="$CFLAGS" CXXFLAGS="$CXXFLAGS" $CONFIGURE_FLAGS
        fi
        make -j"$JOBS"
    ' >/dev/null
code=$(docker wait "$name")
oom=$(docker inspect --format '{{.State.OOMKilled}}' "$name")
docker logs "$name" > "$workdir/build.log" 2>&1
cleanup
took[compile]=$(( $(now) - t0 ))
peak=$(cat "$workdir/memory.peak" 2>/dev/null || echo unknown)
if [ "$code" != 0 ]; then
    echo "compile failed (exit $code, oom killed: $oom, peak memory $peak bytes); last lines of $workdir/build.log:" >&2
    tail -40 "$workdir/build.log" >&2
    exit 1
fi

# runtime image
t0=$(now)
rt="$workdir/runtime-context"
rm -rf "$rt" && mkdir -p "$rt"
cp "$src/src/stellar-core" "$rt/stellar-core"
{
    echo "stellar-core commit $commit ($tag)"
    echo "base $base"
    echo "CC=$cc CXX=$cxx"
    echo "CFLAGS=$cflags"
    echo "CXXFLAGS=$cxxflags"
    echo "configure $configure_flags"
    echo "rust $(sed -n 's/^channel = "\(.*\)"/\1/p' "$src/rust-toolchain.toml")"
} > "$rt/build-info.txt"
docker build -q --target runtime --build-arg "BASE=$base" -t "$image" -f "$twin/Dockerfile" "$rt" >/dev/null
took[image]=$(( $(now) - t0 ))

reported=$(docker run --rm --memory 2g --cpus 2 "$image" version 2>/dev/null | sed -n 's/^stellar-core .*(\([0-9a-f]*\))$/\1/p' | head -1)
if [ "$reported" != "$commit" ]; then
    echo "$image reports commit '$reported', expected $commit" >&2
    exit 1
fi

size=$(docker image inspect --format '{{.Size}}' "$image")
disk=$(du -sb "$workdir" | cut -f1)
echo "image        $image ($(docker image inspect --format '{{.Id}}' "$image"))"
echo "commit       $reported"
echo "jobs         make -j$jobs, cargo $jobs, limit $memory memory, $cpus cpus"
echo "fetch        ${took[fetch]} s"
echo "toolchain    ${took[toolchain]} s"
echo "compile      ${took[compile]} s"
echo "image build  ${took[image]} s"
echo "peak memory  $peak bytes (compile container, cgroup memory.peak)"
echo "work dir     $disk bytes ($workdir)"
echo "image size   $size bytes"
