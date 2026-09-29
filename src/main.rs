//! Reproduction for tokio#8520
//! "UnixStream::shutdown is missing the TCP-side ENOTCONNECTED -> Ok normalization"
//!
//! macOS / Linux: a local peer accepts and then closes with SO_LINGER(0),
//! which sends an RST immediately after the handshake -- the same state the
//! original report hit. We then read() and shutdown() the socket and print
//! every errno, so macOS and Linux can be compared side by side.
//!
//! Windows has no portable RST peer here; it just probes a closed port.

use std::io::Read;
use std::net::{Shutdown, TcpStream};

fn main() {
    println!("== tokio#8520 shutdown repro ==");
    println!("os: {}", std::env::consts::OS);

    #[cfg(unix)]
    unix_case();

    #[cfg(not(unix))]
    windows_case();
}

#[cfg(unix)]
fn unix_case() {
    use std::net::TcpListener;
    use std::os::unix::io::AsRawFd;

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

    // Step 1: read from the reset socket.
    let mut buf = [0u8; 1];
    match stream.read(&mut buf) {
        Ok(0) => println!("read: clean EOF"),
        Ok(n) => println!("read: read {} byte(s)", n),
        Err(e) => println!(
            "read: error {:?} (os errno {})",
            e,
            e.raw_os_error().unwrap_or(0)
        ),
    }

    // Step 2: the actual subject -- shutdown(2) with SHUT_WR, the syscall
    // std::net::TcpStream::shutdown (and tokio's UnixStream::shutdown) wraps.
    let rc = unsafe { libc::shutdown(stream.as_raw_fd(), libc::SHUT_WR) };
    println!(
        "raw shutdown(SHUT_WR): rc={} last_os_error={:?}",
        rc,
        std::io::Error::last_os_error()
    );

    // Step 3: what a std user actually sees.
    match stream.shutdown(Shutdown::Write) {
        Ok(()) => println!("std shutdown(Write): ok"),
        Err(e) => println!(
            "std shutdown(Write): error {:?} (os errno {})",
            e,
            e.raw_os_error().unwrap_or(0)
        ),
    }

    println!("done");
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
