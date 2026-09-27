#!/usr/bin/env bash
# Compila Manifold (C++) como librería estática en third_party/manifold-lib,
# con paralelismo acotado para no agotar la RAM. Ver .cargo/config.toml.
set -euo pipefail

JOBS="${JOBS:-2}"
# Debe coincidir con MANIFOLD_VERSION del build.rs de manifold-csg-sys (ver Cargo.lock).
MANIFOLD_REF="${MANIFOLD_REF:-v3.5.3}"

root="$(cd "$(dirname "$0")/.." && pwd)"
tp="$root/third_party"
mkdir -p "$tp"

if [ ! -f "$tp/manifold-src/CMakeLists.txt" ]; then
    git clone --depth 1 --branch "$MANIFOLD_REF" https://github.com/elalish/manifold.git "$tp/manifold-src"
fi

cmake -S "$tp/manifold-src" -B "$tp/manifold-build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DMANIFOLD_TEST=OFF -DMANIFOLD_PYBIND=OFF -DMANIFOLD_JSBIND=OFF \
    -DMANIFOLD_CBIND=ON -DMANIFOLD_CROSS_SECTION=ON \
    -DMANIFOLD_USE_BUILTIN_CLIPPER2=ON -DMANIFOLD_PAR=OFF \
    -DBUILD_SHARED_LIBS=OFF -DCMAKE_POSITION_INDEPENDENT_CODE=ON
# --config Release: necesario con generadores multi-config (Visual Studio en Windows).
cmake --build "$tp/manifold-build" --config Release -j "$JOBS"

mkdir -p "$tp/manifold-lib"
# .a en Linux/macOS, .lib en Windows (MSVC).
find "$tp/manifold-build" \( -name '*.a' -o -name '*.lib' \) -exec cp {} "$tp/manifold-lib/" \;
ls "$tp/manifold-lib"
