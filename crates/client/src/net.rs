//! The client side of the connection: a reader thread, a writer thread and two channels.

use cubeterra_common::PROTOCOL_VERSION;
use cubeterra_common::net::{read_frame, write_frame};
use cubeterra_common::protocol::{ToClient, ToServer};
use std::io::{BufReader, BufWriter, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

pub enum NetEvent {
    Connected,
    Msg(ToClient),
    Closed(String),
}

pub struct Connection {
    tx: Sender<ToServer>,
    pub rx: Receiver<NetEvent>,
}

impl Connection {
    pub fn send(&self, msg: ToServer) {
        let _ = self.tx.send(msg);
    }
}

/// Connects in the background; the first event is `Connected` or `Closed`.
pub fn connect(addr: String, name: String, view: u8, skin: u8, wake: Option<rune::Waker>) -> Connection {
    let (ev_tx, rx) = mpsc::channel();
    let (tx, out_rx) = mpsc::channel::<ToServer>();
    std::thread::spawn(move || {
        let wake = move || {
            if let Some(w) = &wake {
                w();
            }
        };
        let addr = if addr.contains(':') { addr } else { format!("{addr}:{}", cubeterra_common::DEFAULT_PORT) };
        let stream = addr
            .to_socket_addrs()
            .map_err(|e| e.to_string())
            .and_then(|mut a| a.next().ok_or_else(|| "no address".to_string()))
            .and_then(|a| TcpStream::connect_timeout(&a, Duration::from_secs(6)).map_err(|e| e.to_string()));
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                let _ = ev_tx.send(NetEvent::Closed(format!("Couldn't connect to {addr}: {e}")));
                wake();
                return;
            }
        };
        let _ = stream.set_nodelay(true);
        let Ok(write_half) = stream.try_clone() else { return };
        let _ = ev_tx.send(NetEvent::Connected);
        std::thread::spawn(move || {
            let mut w = BufWriter::new(write_half);
            if write_frame(&mut w, &ToServer::Hello { name, version: PROTOCOL_VERSION, view_distance: view, skin }).is_err() {
                return;
            }
            let _ = w.flush();
            while let Ok(msg) = out_rx.recv() {
                if write_frame(&mut w, &msg).is_err() {
                    return;
                }
                while let Ok(more) = out_rx.try_recv() {
                    if write_frame(&mut w, &more).is_err() {
                        return;
                    }
                }
                if w.flush().is_err() {
                    return;
                }
            }
            let _ = w.get_ref().shutdown(std::net::Shutdown::Both);
        });
        let mut r = BufReader::with_capacity(256 << 10, stream);
        loop {
            match read_frame::<ToClient, _>(&mut r) {
                Ok(msg) => {
                    if ev_tx.send(NetEvent::Msg(msg)).is_err() {
                        return;
                    }
                    wake();
                }
                Err(e) => {
                    let _ = ev_tx.send(NetEvent::Closed(format!("Disconnected: {e}")));
                    wake();
                    return;
                }
            }
        }
    });
    Connection { tx, rx }
}
