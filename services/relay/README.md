# Gameplay relay

The relay forwards authenticated datagrams between VERA clients. It owns neither
simulation nor lobby membership. Clients retain reliable delivery and lockstep.
The shared codec in `../relay-protocol` binds each ticket to a match and player,
authenticates packet contents, rejects replay, and challenges a new source address
before admitting it. Protocol v2 also encrypts gameplay payloads end to end
with XChaCha20-Poly1305. The relay forwards ciphertext and never receives the
match payload key. A new match requires new tickets and a new payload key;
there is no v1 or plaintext fallback.
The HTTPS lobby is trusted to distribute those keys. Encryption protects the
network path and relay; a compromised lobby remains outside that guarantee.

The signed 98-byte ticket contains version 2, match UUID, source index, expiry,
nonce, player UUID and the player's monotonic admission generation. A later
return-confirmed generation retires that player's previous membership; an empty
room is removed. Generation tombstones last for the maximum 300-second ticket
lifetime, so old still-valid grants cannot recreate retired rooms. Renewals keep
the same player, match, generation and payload key. The client gets the payload
key only through authenticated HTTPS, derives a distinct source key, and uses a
fresh OS-random 128-bit nonce prefix plus a checked packet counter. Renewal or
reconstruction generates a new prefix; NAT rebinding retains the counter.

The relay bounds aggregate input at 16,384 datagrams/second and handshakes at
512/second; admitted principals get eight handshakes/second, enough for paired
old/new routes during renewal. The existing 2,048 packet/second principal cap
is joined by actual ciphertext output budgets: 2 MiB/second per sender and
64 MiB/second per room and relay. Whole broadcasts reserve all recipient bytes
before forwarding, preventing partial delivery when a budget is exhausted.
Fixed one-second windows allow ordinary short bursts, including simultaneous
NAT rebinding. Limits count ciphertext, framing and MAC bytes, not just input
packet count. Provider-level protection is still needed for link saturation.

Build from the repository root:

```sh
python -m tools.cargo_run -- build --release -p vera20k-relay
```

Install the executable as `/usr/local/bin/vera20k-relay`, create the unprivileged
`vera20k-relay` user, and install `vera20k-relay.service`. Allow UDP port 5000 in
the cloud firewall. Limit administrative SSH access to the management network.

Generate a 32-byte cryptographic random key outside the repository and encode it
as 64 hexadecimal digits. Store it in root-owned mode-0600
`/etc/vera20k/relay-ticket-key`; the unit supplies it through systemd credentials.
Set the identical key as the Cloudflare Worker's `RELAY_TICKET_KEY` secret. Never
place it in a command argument, source, test result, or client configuration.
Rotate the key during maintenance after existing matches have ended.

Set the Worker's public `RELAY_ADDRESS` to the server's IPv4 address and port.
Clients obtain individual signed tickets and packet keys over authenticated HTTPS;
they never receive the server signing key. Restarting the relay requires matches
to reconnect or fail through the client's existing timeout handling.

The service journal reports aggregate traffic and rejection counts once a minute.
These counts contain no player, endpoint, room identity, ticket or packet contents.
Use them to distinguish rate limits, expired grants and authentication failures
from packet loss and client timeouts.
