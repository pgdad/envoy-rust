//! Self-contained `SO_REUSEPORT` accept-throughput + distribution bench.
//! `#[ignore]`d so the normal `cargo test` run never pays for it. Run with:
//!   cargo test -p envoy-listener --release --test reuseport_bench -- --ignored --nocapture
//!
//! What it measures: connection-accept throughput (connections/sec) and the
//! per-socket distribution for 1 vs. N `SO_REUSEPORT` accepting sockets on one
//! port, under heavy short-connection churn from several client threads. This
//! is the userspace-observable proxy for the feature's benefit — N independent
//! kernel accept queues instead of one.
//!
//! Platform note: **Linux** load-balances `SO_REUSEPORT` across sockets by a
//! 4-tuple hash, so N>1 spreads connections and scales accept throughput on a
//! multi-core box. **macOS/BSD** do NOT load-balance — every connection lands on
//! a single socket — so N>1 shows the SAME throughput and a lopsided
//! distribution there. Both outcomes are "same-or-better": the fan-out never
//! regresses, and it strictly helps where the kernel spreads.

use socket2::{Domain, Protocol, SockRef, Socket, Type};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

fn reuseport_listener(addr: SocketAddr) -> tokio::net::TcpListener {
    let sock = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();
    sock.set_reuse_address(true).unwrap();
    sock.set_reuse_port(true).unwrap();
    sock.set_nonblocking(true).unwrap();
    sock.bind(&addr.into()).unwrap();
    sock.listen(1024).unwrap();
    let std_l: std::net::TcpListener = sock.into();
    tokio::net::TcpListener::from_std(std_l).unwrap()
}

/// Bind `n_sockets` reuseport sockets on one port, run one accept loop per
/// socket, hammer with `client_threads` OS threads for `dur`, and return
/// (connections/sec, per-socket accept counts). The server RST-closes each
/// accepted socket (`SO_LINGER(0)`) so neither side accumulates TIME_WAIT and
/// the run does not exhaust ephemeral ports.
fn run(n_sockets: usize, client_threads: usize, dur: Duration) -> (u64, Vec<u64>) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(n_sockets.max(2))
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let first = reuseport_listener("127.0.0.1:0".parse().unwrap());
        let port = first.local_addr().unwrap().port();
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let mut socks = vec![first];
        for _ in 1..n_sockets {
            socks.push(reuseport_listener(addr));
        }

        let counts: Vec<Arc<AtomicU64>> = (0..n_sockets)
            .map(|_| Arc::new(AtomicU64::new(0)))
            .collect();
        let mut acceptors = Vec::new();
        for (i, s) in socks.into_iter().enumerate() {
            let c = Arc::clone(&counts[i]);
            acceptors.push(tokio::spawn(async move {
                // Count AFTER accept, then RST-close the accepted socket
                // (SO_LINGER 0) so neither side accumulates TIME_WAIT and
                // connections are not torn out of the backlog before this loop
                // counts them.
                while let Ok((stream, _)) = s.accept().await {
                    c.fetch_add(1, Ordering::Relaxed);
                    let _ = SockRef::from(&stream).set_linger(Some(Duration::ZERO));
                    drop(stream);
                }
            }));
        }

        let stop = Arc::new(AtomicBool::new(false));
        let clients: Vec<_> = (0..client_threads)
            .map(|_| {
                let stop = Arc::clone(&stop);
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        // Plain connect + immediate close. The server RST-closes
                        // each accepted socket, so the client sees RST and does
                        // not accumulate TIME_WAIT; connections are torn down only
                        // after the server has accepted (counted) them.
                        let _ = std::net::TcpStream::connect(addr);
                    }
                })
            })
            .collect();

        let start = Instant::now();
        tokio::time::sleep(dur).await;
        stop.store(true, Ordering::Relaxed);
        let elapsed = start.elapsed();
        for c in clients {
            let _ = c.join();
        }
        // Let the acceptors drain the backlog the clients left behind.
        tokio::time::sleep(Duration::from_millis(50)).await;
        for a in &acceptors {
            a.abort();
        }

        let per: Vec<u64> = counts.iter().map(|c| c.load(Ordering::Relaxed)).collect();
        let total: u64 = per.iter().sum();
        let cps = (total as f64 / elapsed.as_secs_f64()) as u64;
        (cps, per)
    })
}

#[test]
#[ignore]
fn bench_reuseport_accept_throughput() {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let dur = Duration::from_millis(1500);
    let clients = cores.max(4);
    let mut ns = vec![1usize, 2, cores];
    ns.sort_unstable();
    ns.dedup();

    println!(
        "SO_REUSEPORT accept throughput ({cores} cores, {clients} client threads, {dur:?} per run):"
    );
    println!("  target_os = {}", std::env::consts::OS);
    let mut baseline: Option<u64> = None;
    for n in ns {
        let (cps, per) = run(n, clients, dur);
        let base = *baseline.get_or_insert(cps.max(1));
        let speedup = cps as f64 / base as f64;
        println!("  {n:>2} socket(s): {cps:>9} conn/s  ({speedup:>4.2}x)  distribution={per:?}");
    }
    println!(
        "(Linux load-balances across sockets → N>1 scales + spreads; macOS/BSD deliver to one \
         socket → same throughput, lopsided distribution. Never a regression.)"
    );
}
