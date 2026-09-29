# tokio-8520 shutdown repro

Reproduction for [tokio#8520](https://github.com/tokio-rs/tokio/issues/8520):
`UnixStream::shutdown` lacks the `NotConnected→Ok` normalization that
`TcpStream::shutdown` has ([#4665](https://github.com/tokio-rs/tokio/issues/4665)).

**Part 1 — Unix domain sockets (the subject of #8520):** the issue's repro
(`UnixStream::pair()`, peer half dropped) plus the variants from the WSL2
comment (shutdown after read / after write / on a never-connected socket).
The question is which platforms surface `ENOTCONN` from `shutdown(2)` here.

**Part 2 — TCP after RST (context, the #4665 race):** a local peer accepts
and closes with `SO_LINGER(0)` (sends an RST right after the handshake), then
we read and shut the socket down. Shows the raw `ENOTCONN` that the TCP
wrapper normalizes away is really reachable on macOS and Linux.

Windows has no Unix domain sockets; it only probes a closed TCP port.

## Run

Every push runs macOS + Linux + Windows automatically (~2 min); you can also
trigger it from the Actions tab → **tokio-8520-shutdown-repro** → **Run
workflow**. See the "Run repro" step of each matrix job.

Locally: `cargo run --release` (Unix/macOS runs both parts; Windows part 1 is skipped).
