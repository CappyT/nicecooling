//! A small in-process MQTT 3.1.1 broker for the tests. It stores what it receives.
//! It does not route messages, keep sessions or check topics.

use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::tcp::OwnedReadHalf;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

#[derive(Clone, Debug)]
pub struct Received {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: u8,
    pub retain: bool,
    pub dup: bool,
    pub pkid: u16,
}

#[derive(Clone, Debug)]
pub struct ConnectInfo {
    pub client_id: String,
    pub clean_session: bool,
    pub keep_alive: u16,
}

enum Out {
    Bytes(Vec<u8>),
    Close,
}

#[derive(Default)]
struct State {
    received: Vec<Received>,
    connects: Vec<ConnectInfo>,
    subscribes: usize,
    hold_acks: bool,
    held: Vec<(mpsc::UnboundedSender<Out>, u16)>,
    drop_publishes: usize,
    conns: Vec<mpsc::UnboundedSender<Out>>,
}

pub struct Broker {
    state: Arc<Mutex<State>>,
    accept: JoinHandle<()>,
}

/// A free local port. The caller binds it later, so the broker can start and stop on one port.
pub fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    l.local_addr().expect("addr").port()
}

impl Broker {
    pub async fn start(port: u16) -> Broker {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("bind broker");
        let state = Arc::new(Mutex::new(State::default()));
        let st = state.clone();
        let accept = tokio::spawn(async move {
            while let Ok((sock, _)) = listener.accept().await {
                tokio::spawn(connection(sock, st.clone()));
            }
        });
        Broker { state, accept }
    }

    /// Closes the listener and every connection, so the port is free again.
    pub async fn stop(self) {
        self.kick();
        self.accept.abort();
        let _ = self.accept.await;
    }

    pub fn kick(&self) {
        for c in self.state.lock().unwrap().conns.drain(..) {
            let _ = c.send(Out::Close);
        }
    }

    pub fn received(&self) -> Vec<Received> {
        self.state.lock().unwrap().received.clone()
    }

    pub fn connects(&self) -> Vec<ConnectInfo> {
        self.state.lock().unwrap().connects.clone()
    }

    pub fn subscribes(&self) -> usize {
        self.state.lock().unwrap().subscribes
    }

    /// While on, the broker keeps each PUBACK of a QoS 1 message until `release_acks`.
    pub fn hold_acks(&self, on: bool) {
        self.state.lock().unwrap().hold_acks = on;
    }

    pub fn release_acks(&self, reverse: bool) {
        let mut held = std::mem::take(&mut self.state.lock().unwrap().held);
        if reverse {
            held.reverse();
        }
        for (tx, pkid) in held {
            let _ = tx.send(Out::Bytes(vec![0x40, 2, (pkid >> 8) as u8, pkid as u8]));
        }
    }

    /// The next `n` QoS 1 messages close their connection with no PUBACK.
    pub fn drop_next_publishes(&self, n: usize) {
        self.state.lock().unwrap().drop_publishes = n;
    }
}

enum In {
    Packet(u8, Vec<u8>),
    Eof,
}

async fn read_packet(rd: &mut OwnedReadHalf) -> std::io::Result<(u8, Vec<u8>)> {
    let first = rd.read_u8().await?;
    let (mut len, mut shift) = (0usize, 0);
    loop {
        let b = rd.read_u8().await?;
        len |= usize::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    let mut body = vec![0; len];
    rd.read_exact(&mut body).await?;
    Ok((first, body))
}

fn take_str(buf: &[u8]) -> (String, &[u8]) {
    let n = usize::from(u16::from_be_bytes([buf[0], buf[1]]));
    (
        String::from_utf8_lossy(&buf[2..2 + n]).into_owned(),
        &buf[2 + n..],
    )
}

fn puback(pkid: u16) -> Out {
    Out::Bytes(vec![0x40, 2, (pkid >> 8) as u8, pkid as u8])
}

async fn connection(sock: tokio::net::TcpStream, st: Arc<Mutex<State>>) {
    let (mut rd, mut wr) = sock.into_split();
    let (in_tx, mut in_rx) = mpsc::unbounded_channel();
    let reader = tokio::spawn(async move {
        loop {
            match read_packet(&mut rd).await {
                Ok((b, body)) => {
                    if in_tx.send(In::Packet(b, body)).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    let _ = in_tx.send(In::Eof);
                    break;
                }
            }
        }
    });
    let (out_tx, mut out_rx) = mpsc::unbounded_channel();
    st.lock().unwrap().conns.push(out_tx.clone());
    loop {
        tokio::select! {
            i = in_rx.recv() => {
                let Some(In::Packet(first, body)) = i else { break };
                match first >> 4 {
                    1 => {
                        let (_proto, rest) = take_str(&body);
                        let (flags, rest) = (rest[1], &rest[2..]);
                        let keep_alive = u16::from_be_bytes([rest[0], rest[1]]);
                        let (client_id, _) = take_str(&rest[2..]);
                        st.lock().unwrap().connects.push(ConnectInfo {
                            client_id,
                            clean_session: flags & 0x02 != 0,
                            keep_alive,
                        });
                        let _ = out_tx.send(Out::Bytes(vec![0x20, 2, 0, 0]));
                    }
                    3 => {
                        let qos = (first >> 1) & 3;
                        let (topic, rest) = take_str(&body);
                        let (pkid, payload) = if qos > 0 {
                            (u16::from_be_bytes([rest[0], rest[1]]), &rest[2..])
                        } else {
                            (0, rest)
                        };
                        let mut s = st.lock().unwrap();
                        s.received.push(Received {
                            topic,
                            payload: payload.to_vec(),
                            qos,
                            retain: first & 1 != 0,
                            dup: first & 8 != 0,
                            pkid,
                        });
                        if qos == 1 {
                            if s.drop_publishes > 0 {
                                s.drop_publishes -= 1;
                                break;
                            }
                            if s.hold_acks {
                                s.held.push((out_tx.clone(), pkid));
                            } else {
                                let _ = out_tx.send(puback(pkid));
                            }
                        }
                    }
                    8 => {
                        st.lock().unwrap().subscribes += 1;
                        let _ = out_tx.send(Out::Bytes(vec![0x90, 3, body[0], body[1], 0]));
                    }
                    12 => {
                        let _ = out_tx.send(Out::Bytes(vec![0xd0, 0]));
                    }
                    14 => break,
                    _ => {}
                }
            }
            o = out_rx.recv() => match o {
                Some(Out::Bytes(b)) => {
                    if wr.write_all(&b).await.is_err() {
                        break;
                    }
                }
                _ => break,
            }
        }
    }
    reader.abort();
    let _ = wr.shutdown().await;
}
