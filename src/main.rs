//! Reproduction for tokio#8520
//! "UnixStream::shutdown lacks the TCP-side ENOTCONNECTED -> Ok normalization (#4665)"
//!
//! Part 1 (the subject of #8520): Unix DOMAIN sockets. Runs the issue's
//! repro (socketpair, peer half dropped) plus the variants tested on WSL2,
//! and prints what shutdown(2) returns on each platform.
//!
//! Part 2 (context, the #4665 race): a TCP peer accepts and then closes
//! with SO_LINGER(0), sending an RST right after the handshake; we then
//! read() and shutdown() the socket. Shows that the raw ENOTCONN which
//! tokio's TCP wrapper normalizes away is really reachable on macOS/Linux.
//!
//! Windows has no Unix domain sockets; it only probes a closed TCP port.

use std::io::Read;
use std::net::{Shutdown, TcpStream};

fn main() {
    println!("== tokio#8520 shutdown repro ==");
    println!("os: {}", std::env::consts::OS);

    #[cfg(unix)]
    {
        println!();
        unix_socket_case(); // part 1: the actual subject of #8520
        println!();
        tcp_rst_case(); // part 2: context, the #4665 race on the TCP path
    }

    #[cfg(not(unix))]
    windows_case();
}

#[cfg(unix)]
fn report(label: &str, res: std::io::Result<()>) {
    match res {
        Ok(()) => println!("{label}: ok"),
        Err(e) => println!(
            "{label}: error {:?} (os errno {})",
            e,
            e.raw_os_error().unwrap_or(0)
        ),
    }
}

#[cfg(unix)]
fn unix_socket_case() {
    use std::io::Write;
    use std::os::unix::io::FromRawFd;
    use std::os::unix::net::UnixStream;

    println!("-- part 1: unix domain sockets (subject of #8520) --");

    // (a) the issue's exact repro: connected pair, then the peer half is gone.
    let (a, b) = UnixStream::pair().expect("pair");
    drop(b);
    report("pair + drop(peer) -> shutdown(Both)", a.shutdown(Shutdown::Both));

    // (b) shutdown after a read that hit the reset.
    let (mut a, b) = UnixStream::pair().expect("pair");
    drop(b);
    let mut buf = [0u8; 1];
    match a.read(&mut buf) {
        Ok(0) => println!("  read after drop: clean EOF"),
        Ok(n) => println!("  read after drop: read {} byte(s)", n),
        Err(e) => println!(
            "  read after drop: error {:?} (os errno {})",
            e,
            e.raw_os_error().unwrap_or(0)
        ),
    }
    report("read-then shutdown(Both)", a.shutdown(Shutdown::Both));

    // (c) shutdown after a write post-drop.
    let (mut a, b) = UnixStream::pair().expect("pair");
    drop(b);
    match a.write(&[1]) {
        Ok(n) => println!("  write after drop: wrote {} byte(s)", n),
        Err(e) => println!(
            "  write after drop: error {:?} (os errno {})",
            e,
            e.raw_os_error().unwrap_or(0)
        ),
    }
    report("write-then shutdown(Both)", a.shutdown(Shutdown::Both));

    // (d) never-connected socket: raw fd, never bound nor connected.
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    assert!(fd >= 0, "socket(AF_UNIX)");
    let sock = unsafe { UnixStream::from_raw_fd(fd) };
    report("never-connected shutdown(Both)", sock.shutdown(Shutdown::Both));
}

#[cfg(unix)]
fn tcp_rst_case() {
    use std::net::TcpListener;
    use std::os::unix::io::AsRawFd;

    println!("-- part 2: TCP after RST (context: the #4665 race) --");

    // Peer thread: accept one connection, then reset it.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let peer = std::thread::spawn(move || {
        let (sock, _) = listener.accept().expect("accept");
        // SO_LINGER with a zero timeout: close() sends RST instead of FIN.
        let linger = libc::linger { l_onoff: 1, l_linger: 0 };
        let rc = unsafe {
            libc::setsockopt(
                sock.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_LINGER,
                std::ptr::addr_of!(linger).cast(),
                std::mem::size_of::<libc::linger>() as libc::socklen_t,
            )
        };
        assert_eq!(rc, 0, "setsockopt(SO_LINGER)");
        drop(sock);
    });

    let mut stream = TcpStream::connect(addr).expect("connect");
    peer.join().expect("peer thread");

    let mut buf = [0u8; 1];
    match stream.read(&mut buf) {
        Ok(0) => println!("  read: clean EOF"),
        Ok(n) => println!("  read: read {} byte(s)", n),
        Err(e) => println!(
            "  read: error {:?} (os errno {})",
            e,
            e.raw_os_error().unwrap_or(0)
        ),
    }

    // The syscall std::net::TcpStream::shutdown (and tokio's TcpStream
    // wrapper, which normalizes ENOTCONN -> Ok) ultimately runs.
    let rc = unsafe { libc::shutdown(stream.as_raw_fd(), libc::SHUT_WR) };
    println!(
        "  raw shutdown(SHUT_WR): rc={} last_os_error={:?}",
        rc,
        std::io::Error::last_os_error()
    );
    println!("  done");
}

#[cfg(not(unix))]
fn windows_case() {
    // Filler row only: nothing listens on port 1.
    match TcpStream::connect("127.0.0.1:1") {
        Ok(mut s) => {
            println!("connect: ok (unexpected)");
            let mut buf = [0u8; 1];
            match s.read(&mut buf) {
                Ok(0) => println!("read: clean EOF"),
                Ok(n) => println!("read: read {} byte(s)", n),
                Err(e) => println!("read: error {:?} (os errno {})", e, e.raw_os_error().unwrap_or(0)),
            }
            match s.shutdown(Shutdown::Both) {
                Ok(()) => println!("shutdown(Both): ok"),
                Err(e) => println!("shutdown(Both): error {:?} (os errno {})", e, e.raw_os_error().unwrap_or(0)),
            }
        }
        Err(e) => println!(
            "connect: error {:?} (os errno {}) -- nothing further to probe on this platform",
            e,
            e.raw_os_error().unwrap_or(0)
        ),
    }
    println!("done");
}
