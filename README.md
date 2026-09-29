# tokio-8520 shutdown repro

Reproduction for [tokio#8520](https://github.com/tokio-rs/tokio/issues/8520):
after the peer resets the connection (RST), `shutdown(2)` on macOS returns
`ENOTCONN`, which surfaces through `UnixStream::shutdown` / std
`TcpStream::shutdown`, while the TCP code path normalizes that errno to `Ok`.

On macOS/Linux the program connects to a local listener whose peer closes with
`SO_LINGER(0)` (sends an RST right after the handshake), then reads and shuts
the socket down, printing every errno along the way. On Windows it probes a
closed port as a filler row.

## Run

Actions tab → **tokio-8520-shutdown-repro** → **Run workflow**
(runs macOS + Linux + Windows in one go, ~2 min each; see the "Run repro"
step of each matrix job).

Locally: `cargo run --release`
