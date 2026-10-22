#!/usr/bin/env bash
set -euo pipefail
switch_port_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$switch_port_root"
gpu=false
profile=""
artifact=vera20k-core
if [[ $# == 1 && $1 == --gpu ]]; then
    gpu=true
    profile=master-
    artifact=vera20k-gpu
elif [[ $# != 0 ]]; then
    echo "Usage: run_eden.sh [--gpu]" >&2
    exit 2
fi
export XDG_DATA_HOME="$switch_port_root/target/emulator/${profile}xdg-data"
export XDG_CONFIG_HOME="$switch_port_root/target/emulator/${profile}xdg-config"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME/eden/"{sdmc,nand,log,keys,load,dump,shader,screenshots,play_time}
eden_keys="$XDG_DATA_HOME/eden/keys/prod.keys"
if [[ ! -s "$eden_keys" ]]; then
    for local_keys in "${SWITCH_KEYS:-}" "$HOME/.switch/prod.keys" "$HOME/.local/share/eden/keys/prod.keys"; do
        if [[ -n "$local_keys" && -s "$local_keys" ]]; then
            install -m 600 -- "$local_keys" "$eden_keys"
            break
        fi
    done
fi
if [[ ! -s "$eden_keys" ]]; then
    echo "No local prod.keys found. Set SWITCH_KEYS to your device key file." >&2
    exit 1
fi
chmod 600 -- "$eden_keys"
if $gpu; then
    eden_app="${EDEN_APP:-$switch_port_root/../../../../toolchains/switch/eden-src/build-master-cli/bin/eden-cli}"
    exec "$eden_app" -g "$switch_port_root/dist/$artifact.nro"
fi
eden_app="${EDEN_APP:-$switch_port_root/../../../../toolchains/switch/emulators/eden-0.2.1/squashfs-root/AppRun}"
exec "$eden_app" eden-cli -g "$switch_port_root/dist/$artifact.nro"
