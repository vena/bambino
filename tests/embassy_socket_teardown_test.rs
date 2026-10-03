//! Host-side check that a dropped embassy raw stream tells the peer the connection ended.
//!
//! **What it guards.** On the embassy backend, every dropped connection must be closed cleanly,
//! with a FIN, rather than vanish without the peer being told. embassy-net's own `TcpClient`
//! cannot do that, and the compiler cannot catch an embassy-net change that brings it back, so
//! this test watches the wire instead: two real embassy-net stacks joined by an in-memory
//! Ethernet link, one dialing the other through bambino's `EmbassyRawStreamFactory`, and the
//! listening side checking that it sees the dial's FIN once the stream is dropped.
//!
//! **Why this runs on the host.** `embassy-net` needs only a `Driver` and a time source.
//! `InMemoryLink` below is the driver; `embassy-time`'s `std` feature (a dev-dependency)
//! supplies the clock. `embassy_futures::block_on` polls the two stack runners and the test
//! body together on one thread. Run it with:
//!
//! ```sh
//! cargo test --no-default-features --features "embassy,std" --test embassy_socket_teardown_test
//! ```
//!
//! **What this covers and what it does not.** It covers what bambino's embassy backend puts on
//! the wire when a stream is dropped. It does not cover how a printer reacts; that is
//! `embassy-hw-probe`'s job against real hardware.
#![cfg(all(feature = "embassy", feature = "std"))]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::task::{Context, Waker};

use bambino::io::embassy::{EmbassyRawStreamFactory, EmbassySocketBuffers, EmbassySocketPool};
use bambino::io::{RawStreamFactory, SocketError};
use embassy_futures::select::{Either, Either3, select, select3};
use embassy_net::driver::{Capabilities, Driver, HardwareAddress, LinkState, RxToken, TxToken};
use embassy_net::tcp::TcpSocket;
use embassy_net::{Config, Ipv4Address, Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4};
use embassy_time::{Duration, with_timeout};

/// The port the listening stack accepts on.
const PORT: u16 = 990;
/// How long the listening side waits for the FIN. Generous for an in-memory link, where the
/// FIN, when it is sent at all, arrives within a few stack polls.
const FIN_BOUND: Duration = Duration::from_secs(2);
/// The dialing side's address, and the listening side's.
const DIALER_IP: Ipv4Address = Ipv4Address::new(10, 0, 0, 1);
const LISTENER_IP: Ipv4Address = Ipv4Address::new(10, 0, 0, 2);

/// Frames in flight in each direction, plus each side's receive waker.
#[derive(Default)]
struct Wire {
    queues: [VecDeque<Vec<u8>>; 2],
    wakers: [Option<Waker>; 2],
}

/// One end of an in-memory Ethernet link: frames it transmits arrive at the other end.
struct InMemoryLink {
    wire: Rc<RefCell<Wire>>,
    /// This end's index; it receives from `queues[side]` and sends to `queues[1 - side]`.
    side: usize,
    mac: [u8; 6],
}

fn link_pair() -> (InMemoryLink, InMemoryLink) {
    let wire = Rc::new(RefCell::new(Wire::default()));
    (
        InMemoryLink {
            wire: wire.clone(),
            side: 0,
            mac: [0x02, 0, 0, 0, 0, 1],
        },
        InMemoryLink {
            wire,
            side: 1,
            mac: [0x02, 0, 0, 0, 0, 2],
        },
    )
}

struct Rx(Vec<u8>);

impl RxToken for Rx {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(mut self, f: F) -> R {
        f(&mut self.0)
    }
}

struct Tx {
    wire: Rc<RefCell<Wire>>,
    to: usize,
}

impl TxToken for Tx {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, len: usize, f: F) -> R {
        let mut frame = vec![0; len];
        let result = f(&mut frame);
        let mut wire = self.wire.borrow_mut();
        wire.queues[self.to].push_back(frame);
        if let Some(waker) = wire.wakers[self.to].take() {
            waker.wake();
        }
        result
    }
}

impl Driver for InMemoryLink {
    type RxToken<'a> = Rx;
    type TxToken<'a> = Tx;

    fn receive(&mut self, cx: &mut Context) -> Option<(Rx, Tx)> {
        let mut wire = self.wire.borrow_mut();
        match wire.queues[self.side].pop_front() {
            Some(frame) => Some((
                Rx(frame),
                Tx {
                    wire: self.wire.clone(),
                    to: 1 - self.side,
                },
            )),
            None => {
                wire.wakers[self.side] = Some(cx.waker().clone());
                None
            }
        }
    }

    fn transmit(&mut self, _cx: &mut Context) -> Option<Tx> {
        Some(Tx {
            wire: self.wire.clone(),
            to: 1 - self.side,
        })
    }

    fn link_state(&mut self, _cx: &mut Context) -> LinkState {
        LinkState::Up
    }

    fn capabilities(&self) -> Capabilities {
        let mut caps = Capabilities::default();
        caps.max_transmission_unit = 1514;
        caps
    }

    fn hardware_address(&self) -> HardwareAddress {
        HardwareAddress::Ethernet(self.mac)
    }
}

fn leak<T>(value: T) -> &'static mut T {
    Box::leak(Box::new(value))
}

fn stack(
    link: InMemoryLink,
    ip: Ipv4Address,
    seed: u64,
) -> (Stack<'static>, Runner<'static, InMemoryLink>) {
    let config = Config::ipv4_static(StaticConfigV4 {
        address: Ipv4Cidr::new(ip, 24),
        gateway: None,
        dns_servers: Default::default(),
    });
    embassy_net::new(link, config, leak(StackResources::<4>::new()), seed)
}

/// Runs `body` with both stacks up and their runners polled alongside it.
fn with_two_stacks<F, Fut, T>(body: F) -> T
where
    F: FnOnce(Stack<'static>, Stack<'static>) -> Fut,
    Fut: Future<Output = T>,
{
    let (dialer_link, listener_link) = link_pair();
    let (dialer, mut dialer_runner) = stack(dialer_link, DIALER_IP, 1);
    let (listener, mut listener_runner) = stack(listener_link, LISTENER_IP, 2);
    let body = async move {
        dialer.wait_config_up().await;
        listener.wait_config_up().await;
        body(dialer, listener).await
    };
    match embassy_futures::block_on(select3(dialer_runner.run(), listener_runner.run(), body)) {
        Either3::Third(result) => result,
        Either3::First(never) | Either3::Second(never) => match never {},
    }
}

/// How the connection ended, as seen by the listening side once the dialing side dropped it.
#[derive(Debug, PartialEq)]
enum PeerSaw {
    /// The read returned 0: a FIN arrived.
    Fin,
    /// The read failed: an RST arrived.
    Reset,
    /// Nothing arrived within `FIN_BOUND`: the connection vanished silently.
    Nothing,
}

/// Accepts one connection on `listener`, lets `dial_and_drop` connect and drop it, and reports
/// how many bytes arrived before the connection ended and how it ended. Leaves the listening
/// socket open, so the dialing side's socket stays in `FinWait2`.
async fn peer_view_of_drop<D: Future<Output = ()>>(
    listener: Stack<'static>,
    dial_and_drop: D,
) -> (usize, PeerSaw) {
    let mut socket = TcpSocket::new(listener, leak([0u8; 1024]), leak([0u8; 1024]));
    let (accepted, ()) = embassy_futures::join::join(socket.accept(PORT), dial_and_drop).await;
    accepted.expect("listener accepts the dial");
    let mut received = 0;
    let mut buf = [0u8; 64];
    loop {
        match with_timeout(FIN_BOUND, socket.read(&mut buf)).await {
            Ok(Ok(0)) => return (received, PeerSaw::Fin),
            Ok(Ok(n)) => received += n,
            Ok(Err(_)) => return (received, PeerSaw::Reset),
            Err(_) => return (received, PeerSaw::Nothing),
        }
    }
}

/// A one-socket pool on `dialer` and a factory over it. The pool task isn't spawned: a dropped
/// stream's FIN must not depend on it.
fn one_socket_factory(dialer: Stack<'static>) -> EmbassyRawStreamFactory {
    let pool = EmbassySocketPool::new(dialer, leak(EmbassySocketBuffers::<1, 1024, 1024>::new()));
    EmbassyRawStreamFactory::new(pool)
}

/// Dials the listener, bounded so a pool that never frees a socket fails the test rather than
/// hanging it.
async fn dial(factory: EmbassyRawStreamFactory) -> bambino::io::embassy::EmbassyTcpStream {
    with_timeout(FIN_BOUND, factory.dial(&LISTENER_IP.to_string(), PORT))
        .await
        .expect("dial finishes within FIN_BOUND")
        .expect("dial over the in-memory link")
}

/// The harness can see a FIN at all: a plain `TcpSocket` closed and flushed by the dialing side.
/// If this fails, the test bed is broken, not bambino.
#[test]
fn harness_sees_a_plain_sockets_fin() {
    let saw = with_two_stacks(|dialer, listener| {
        peer_view_of_drop(listener, async move {
            let mut socket = TcpSocket::new(dialer, leak([0u8; 1024]), leak([0u8; 1024]));
            socket
                .connect((LISTENER_IP, PORT))
                .await
                .expect("dial over the in-memory link");
            socket.close();
            socket.flush().await.expect("FIN acked");
        })
    });
    assert_eq!(saw, (0, PeerSaw::Fin));
}

/// A stream from `EmbassyRawStreamFactory`, dropped without sending a byte (what happens when
/// `Session::new` fails), must reach the peer as a FIN.
#[test]
fn factory_stream_dropped_unused_sends_fin() {
    let saw = with_two_stacks(|dialer, listener| {
        let factory = one_socket_factory(dialer);
        peer_view_of_drop(listener, async move { drop(dial(factory).await) })
    });
    assert_eq!(saw, (0, PeerSaw::Fin));
}

/// A stream dropped after writing must deliver what it wrote, then a FIN.
#[test]
fn factory_stream_dropped_after_write_sends_data_then_fin() {
    use embedded_io_async::Write;
    let saw = with_two_stacks(|dialer, listener| {
        let factory = one_socket_factory(dialer);
        peer_view_of_drop(listener, async move {
            let mut stream = dial(factory).await;
            stream.write_all(b"220 hello").await.expect("write");
            drop(stream);
        })
    });
    assert_eq!(saw, (9, PeerSaw::Fin));
}

/// A dial cancelled while connecting hands its socket back: the one-socket pool can dial again,
/// and that stream's drop still sends a FIN.
#[test]
fn cancelled_dial_returns_its_socket() {
    let saw = with_two_stacks(|dialer, listener| {
        let factory = one_socket_factory(dialer);
        async move {
            // `select` polls the dial first; it leases, starts connecting and returns `Pending`,
            // then the ready branch wins and the dial is dropped in `SynSent`.
            let cancelled = select(
                factory.dial(&LISTENER_IP.to_string(), PORT),
                core::future::ready(()),
            )
            .await;
            assert!(
                matches!(cancelled, Either::Second(())),
                "dial was not cancelled"
            );
            peer_view_of_drop(listener, async move { drop(dial(factory).await) }).await
        }
    });
    assert_eq!(saw, (0, PeerSaw::Fin));
}

/// With its only socket held by a live stream, the pool fails a dial at once instead of waiting.
#[test]
fn dial_with_every_socket_leased_is_resource_exhausted() {
    let second = with_two_stacks(|dialer, listener| {
        let factory = one_socket_factory(dialer);
        async move {
            let mut socket = TcpSocket::new(listener, leak([0u8; 1024]), leak([0u8; 1024]));
            let (accepted, held) =
                embassy_futures::join::join(socket.accept(PORT), dial(factory)).await;
            accepted.expect("listener accepts the dial");
            let second = with_timeout(FIN_BOUND, factory.dial(&LISTENER_IP.to_string(), PORT))
                .await
                .expect("dial fails at once rather than waiting");
            drop(held);
            second.err()
        }
    });
    assert_eq!(second, Some(SocketError::ResourceExhausted));
}

/// Once the peer has closed its side too, the socket (now in `TimeWait`) is dialed again.
#[test]
fn socket_is_reused_after_the_peer_closes() {
    let second = with_two_stacks(|dialer, listener| {
        let factory = one_socket_factory(dialer);
        async move {
            let mut first = TcpSocket::new(listener, leak([0u8; 1024]), leak([0u8; 1024]));
            let (accepted, stream) =
                embassy_futures::join::join(first.accept(PORT), dial(factory)).await;
            accepted.expect("listener accepts the first dial");
            drop(stream);
            let mut buf = [0u8; 16];
            let fin = with_timeout(FIN_BOUND, first.read(&mut buf)).await;
            assert_eq!(fin, Ok(Ok(0)), "first stream's FIN");
            first.close();
            // This dial waits for the first connection to finish closing, then reuses its socket.
            let mut second = TcpSocket::new(listener, leak([0u8; 1024]), leak([0u8; 1024]));
            let (accepted, redial) = embassy_futures::join::join(
                second.accept(PORT),
                with_timeout(FIN_BOUND, factory.dial(&LISTENER_IP.to_string(), PORT)),
            )
            .await;
            accepted.expect("listener accepts the second dial");
            redial.map(|r| r.is_ok())
        }
    });
    assert_eq!(second, Ok(true));
}
