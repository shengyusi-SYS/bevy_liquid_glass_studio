#!/usr/bin/env bash
set -euo pipefail

if [[ ${1:-} == -h || ${1:-} == --help ]]; then
    printf 'Usage: %s [ui_panel|minimal] [cargo flags] [-- app flags]\n' "$0"
    printf 'Defaults: ui_panel and -j2. Pass --features ui-debug to debug the panel.\n'
    exit 0
fi

example=ui_panel
case ${1:-} in
    ui_panel|minimal) example=$1; shift ;;
    ''|-*) ;;
    *) printf 'Unknown example: %s (choose ui_panel or minimal)\n' "$1" >&2; exit 2 ;;
esac

cargo_flags=()
while (($#)) && [[ $1 != -- ]]; do
    cargo_flags+=("$1")
    shift
done
if (($#)); then shift; fi
app_flags=("$@")

jobs=(-j2)
for flag in "${cargo_flags[@]}"; do
    case $flag in -j|-j?*|--jobs|--jobs=*) jobs=() ;; esac
done
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$project_dir"
command=(cargo run "${jobs[@]}" --example "$example" "${cargo_flags[@]}" -- "${app_flags[@]}")

kernel_release=''
if [[ -r /proc/sys/kernel/osrelease ]]; then kernel_release=$(</proc/sys/kernel/osrelease); fi
if [[ $kernel_release == *[Mm]icrosoft* ]] ||
    [[ -e /dev/dxg && ( -n ${WSL_DISTRO_NAME:-} || -n ${WSL_INTEROP:-} ) ]]; then
    printf 'WSLg preview: D3D12 through GL/X11; adapter preference: %s\n' "${MESA_D3D12_DEFAULT_ADAPTER_NAME:-Mesa default}"
    exec env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET \
        GALLIUM_DRIVER=d3d12 WGPU_BACKEND=gl WINIT_UNIX_BACKEND=x11 "${command[@]}"
fi
exec "${command[@]}"
