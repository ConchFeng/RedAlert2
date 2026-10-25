#!/usr/bin/env bash
set -euo pipefail
switch_port_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$switch_port_root"
gpu=false
profile=""
artifact=vera20k-core
if [[ $# == 1 && ( $1 == --gpu || $1 == --client ) ]]; then
    gpu=true
    profile=master-
    if [[ $1 == --client ]]; then
        artifact=vera20k-client
    else
        artifact=vera20k-gpu
    fi
elif [[ $# != 0 ]]; then
    echo "Usage: run_eden.sh [--gpu|--client]" >&2
    exit 2
fi
export XDG_DATA_HOME="$switch_port_root/target/emulator/${profile}xdg-data"
export XDG_CONFIG_HOME="$switch_port_root/target/emulator/${profile}xdg-config"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME/eden/"{sdmc,nand,log,keys,load,dump,shader,screenshots,play_time}
config_file="$XDG_CONFIG_HOME/eden/sdl2-config.ini"
if [[ ! -f "$config_file" ]]; then
    mkdir -p -- "$(dirname -- "$config_file")"
    printf '[Controls]\ntouch_device="min_x:0,min_y:0,max_x:1279,max_y:719"\n' >"$config_file"
fi
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
    map_target="$XDG_DATA_HOME/eden/sdmc/switch/vera20k/maps/ai_dred_bombard.map"
    map_source="$switch_port_root/../../tools/map_observation/examples/ai_dred_bombard.map"
    mkdir -p -- "$(dirname -- "$map_target")"
    install -m 644 -- "$map_source" "$map_target"
    retail_root="${VERA_RETAIL_DIR:-$HOME/RA2}"
    if [[ -z "$retail_root" || ! -f "$retail_root/ra2md.mix" || ! -f "$retail_root/ra2.mix" ]]; then
        echo "No Yuri's Revenge retail directory found. Set VERA_RETAIL_DIR to the folder with ra2md.mix and ra2.mix." >&2
        exit 1
    fi
    retail_target="$XDG_DATA_HOME/eden/sdmc/game"
    mkdir -p -- "$retail_target"
    while IFS= read -r -d '' retail_file; do
        if [[ ! -s "$retail_file" ]]; then
            continue
        fi
        install -m 644 -- "$retail_file" "$retail_target/"
    done < <(find "$retail_root" -maxdepth 1 -type f \( -iname '*.mix' -o -iname '*.ini' -o -iname '*.exe' -o -iname '*.csf' -o -iname '*.fnt' -o -iname '*.dat' -o -iname '*.pal' \) -print0)
    for empty_placeholder in movies01.mix movmd03.mix; do
        if [[ -f "$retail_target/$empty_placeholder" && ! -s "$retail_target/$empty_placeholder" ]]; then
            rm -f -- "$retail_target/$empty_placeholder"
        fi
    done
    for required_file in ra2md.mix ra2.mix; do
        if [[ ! -f "$retail_target/$required_file" ]] &&
            ! find "$retail_target" -maxdepth 1 -type f -iname "$required_file" -print -quit | grep -q .; then
            echo "Retail resource staging missed $required_file" >&2
            exit 1
        fi
    done
    if [[ $artifact == vera20k-client ]]; then
        client_config="$XDG_DATA_HOME/eden/sdmc/switch/vera20k/config.toml"
        if [[ ! -e "$client_config" ]]; then
            printf '[paths]\nra2_dir = "../../game"\nsettings_dir = "."\n' >"$client_config"
        fi
    fi
    eden_app="${EDEN_APP:-$switch_port_root/../../../../toolchains/switch/eden-src/build-master-cli/bin/eden-cli}"
    exec "$eden_app" -g "$switch_port_root/dist/$artifact.nro"
fi
eden_app="${EDEN_APP:-$switch_port_root/../../../../toolchains/switch/emulators/eden-0.2.1/squashfs-root/AppRun}"
exec "$eden_app" eden-cli -g "$switch_port_root/dist/$artifact.nro"
