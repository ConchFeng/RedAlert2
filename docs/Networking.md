# Multiplayer behavior and ownership

Current macOS and Linux source builds provide LAN and online multiplayer through
the normal game menus. Both paths use the existing
owners for session setup, synchronized commands, reliable UDP delivery, frame
pacing, checksums, disconnects, results and rematches. The function-level
active-retail `gamemd.exe` parity audit remains separate from runtime validation.

Legacy IPX sockets and the IPX protocol are excluded. Shared behavior in
IPX-named classes remains relevant. Retail's Network menu constructs the IPX
interface (`52E3D2`); VERA uses IPv4 UDP for this LAN entry point. That replacement,
including limited broadcast on port 1234, is an explicit transport adaptation.
Online play uses the Cloudflare HTTPS lobby and an authenticated UDP gameplay
relay on DigitalOcean. Relay admission handles client source-address changes;
clients retain reliable delivery and simulation authority. Matches are between
VERA20k clients; legacy IPX and gamemd wire compatibility are not supported.

## Current production path

The Network menu enters the LAN lobby. The physical service handles discovery,
join, options, roster, readiness and GO, then transfers the same socket pair into
the running match. The Internet menu's Custom Match path uses normal account
creation/login, game discovery, host settings and guest acceptance. Its HTTPS
service supplies match-specific relay credentials before the same loading and
command handoff. Both paths transfer selected authored maps or generated
seed/preview data and verify build, rules and effective-map agreement before start.

The app services reliable messages and intermediate progress while loading, and
continues servicing transport during frame waits. Gameplay uses the same
OutList/DoList owners as offline play; the transport does not own a second
simulation command queue.

| Responsibility | Authoritative owner |
|---|---|
| Pending and scheduled simulation commands, native event dispatch | `sim::command_queue`, `sim::world::network_commands` |
| Event compression/decompression and persistent caches | `net::event_packet` |
| Packet construction, cumulative counts and peer frame progress | `net::command_exchange` |
| Shared reliable send queue, retransmission and response-time history | `net::reliable` |
| Global application message admission, acknowledgements and retirement | `net::global` |
| UDP sockets, datagram checksum and retained raw send attempts | `net::udp` |
| Startup agreement, native send cadence and command wait | `net::live_commands` |
| LAN packet layouts and options parsing | `net::lan_messages` |
| Participant membership, admitted endpoints, launch state and checksum history | `app::match_runtime::session::MatchSession` |
| Discovery projection and physical LAN lobby/setup service | `app::lan_service` |
| Online account, lobby and relay-admission adapter | `app::shell_wol::live_service` |
| Shared map source, generated staging and verified transfer admission | `map::source`, `app::shell_network_setup::maps` |
| Setup UI, map admission and pending handoff | `app::shell_network_setup` |
| Production frame boundary and post-Logic continuation | `app::match_runtime::sim_tick` and `sim::world` |
| Connection dialogs, votes and participant cleanup | `app::match_runtime::network_match` |

Simulation never reads a socket. The app lends the transport-backed command
boundary to the simulation at the established multiplayer queue phase. A pending
network wait saves the post-Logic continuation; resuming it does not repeat Logic
or input admission. Presentation and local input state do not select the shared
simulation update order.

The service retains admitted setup traffic in the global receive queue during
the asynchronous lobby-to-setup transition. Loading retains the frozen setup,
so that transition guard does not block loading progress. Row bindings and
participant membership remain distinct: replacing a human row sends the native
kick, while its departure removes the admitted participant and endpoint.

## Native behavior and deliberate bounds

- Event packets use the native layouts and persistent compression caches from
  `64B4D0`, `64B660` and `64BDD0`. The packet boundary rejects a semantic event
  without a supported raw command producer rather than inventing a wire form.
- `649CA0` mirrors the local OutList before broadcasting it. Send failures do not
  undo that mirror; the reached native wait and recovery paths decide what
  happens next. Transport queues retain bytes, not gameplay decisions.
- Private and global connections share the `48C3E0` send/retry owner. Global
  admission and FIFO retirement preserve `53FCB0`, `53FF10` and `5400D0` behavior.
- Ordinary acknowledgements use the primary socket even after an alternate-socket
  receive. The native exception is the separate CDEF router connection, which is
  not implemented here.
- Raw UDP send attempts follow `7B23B0`/`7B3AA0`: hard errors retain the exact FIFO
  head; WouldBlock retires that raw attempt. The reliable owner still controls its
  retry. Portable polling supplies write opportunities in place of Win32 events.
- LAN pre/post-GO timing preserves the distinct host and guest paths, including
  conditional primary-global response-time reads and signed versus unsigned
  arithmetic. Guest GO completion gives the primary global channel its separate
  timing. After gameplay peers are installed, `52EA18`/`52EA4F` select independent
  private and primary-global retry intervals.
- The initial successful player barrier resets response history through the
  shared `48BAA0` owner. This preserves pending traffic, original send timestamps,
  sequence counters and receive history. WOL's periodic reset scheduling and its
  additional channels remain outside the current LAN path.
- Ordinary waits use the private response maximum from `542450`; primary-global
  acknowledgement history is excluded. The native optional extra channel at
  manager `+50` remains an unimplemented dependency.
- Setup Back retains transport history within a lobby visit. A fresh post-match
  visit resets queues, sequence history, observations and lobby timing while
  retaining the portable socket bindings.

## Validation and native audit

Two consecutive online matches passed functional validation with 30 independent
client processes and more than 20,000 active mobile units per match. The clients
ran across seven machines: two local macOS clients on one Mac and 28 Linux clients
cohosted on six VMs. The run covered real Move/Stop input, matching shared
simulation checksums and hashes, both results screens and lobby returns, a rematch
in the same client processes, and final Main Menu return.

The Linux clients used software rendering. Sampled host traces showed approximately
0.74 simulation frames per wall-clock second in each match. These samples
measure observed simulation progress, not display FPS or the selected TIMING rate.
This establishes functional scale behavior; smooth, real-time performance remains
unproven. Exhaustive fault coverage and Windows multiplayer validation are not
claimed here.

The native-function audit continues through the existing owners, including wider
gameplay command producers and cooperative handoffs. Native extra-channel/router
behavior, interface selection and local MoveFlash drawing retain their separate
audit scope. VERA's IPv4/relay transport and bounded command admission do not claim
legacy wire compatibility or exhaustive gamemd behavior.

## Reproducible validation tools

The `tools/storage_oracle/lan_*` harnesses execute original instruction slices
for the packet, checksum, queue, timing and message controls their Rust tests
consume. Metadata states the input boundaries and substitutions. These small
corpora establish only their covered cases.

The `lan-live-host` and `lan-live-guest` shell-capture targets drive independent
App processes through the ordinary LAN service and gameplay frame boundary.
They require isolated mutable copies of the retail game directory, each with a
`.vera20k-lan-capture-root` containing `VERA20k isolated shell capture fixture`
followed by a newline. Set each process's `RA2_DIR` to its own copy and keep its
own `config.toml`.

After building the normal app with `--profile fast`, run the host with
`--shell-capture lan-live-host --width 800 --height 600 --cursor-x 400
--cursor-y 300 --output <new-host-directory> --lan-bind 127.0.0.1:32100
--lan-peer 127.0.0.1:32110`. Run the guest with `lan-live-guest`, a separate output
directory, and the bind/peer endpoints exchanged. The output manifest's
`lan.rounds[n].runtime.native_checksums` records per-frame checksums for each
round. The live targets drive real Move/Stop input, results, lobby rematch and
Main Menu return across two games.
This loopback example alone does not establish separate-machine or scale coverage.
Normal players do not need these diagnostic flags. Validation results belong in
the task's validation records, not the tree.

The `lan-battle-host` and `lan-battle-guest` targets use the same isolated setup
and endpoints. Both select Americans through setup, deploy their AMCV (host D
key, guest self-click), and produce a GAPOWR. The input driver holds, resumes,
cancels with a refund check, then builds and places a fresh product while
retaining its paid identity. Both clients must observe both completed power
plants. The manifest retains 7,200 frame checksums and the observed production
actions. This is a bounded opening check with a 300-second default diagnostic
budget; simulation timing and network timeouts still belong to production.
