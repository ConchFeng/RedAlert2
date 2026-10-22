#!/usr/bin/env bash
set -euo pipefail
switch_port_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$switch_port_root"
export XDG_DATA_HOME="$switch_port_root/target/emulator/xdg-data"
export XDG_CONFIG_HOME="$switch_port_root/target/emulator/xdg-config"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME/eden/"{sdmc,nand,log,keys,load,dump,shader,screenshots}
eden_app="${EDEN_APP:-$switch_port_root/../../../../toolchains/switch/emulators/eden-0.2.1/squashfs-root/AppRun}"
exec "$eden_app" eden-cli -g "$switch_port_root/dist/vera20k-core.nro"
