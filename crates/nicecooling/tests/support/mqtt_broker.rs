//! A small MQTT 3.1.1 broker for the tests of the MQTT client. It listens on a local port and
//! keeps what each connection sends. It answers CONNECT, PUBLISH of QoS 0 and 1, SUBSCRIBE,
//! PINGREQ and DISCONNECT. It routes no message and keeps no session. It can hold the PUBACK
//! messages, delay the CONNACK, send a PUBLISH of its own, close the connections, and offer TLS
//! with the certificate of `TestPki`. It uses threads and blocking sockets and needs no runtime:
//! each connection thread polls its socket with a short read timeout.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rustls::{ServerConfig, ServerConnection, StreamOwned};

/// How long a connection thread waits for bytes before it looks at its other work.
const POLL: Duration = Duration::from_millis(10);

/// The last will of a CONNECT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Will {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: u8,
    pub retain: bool,
}

/// What one CONNECT carried.
#[derive(Clone, Debug)]
pub struct ConnectInfo {
    pub client_id: String,
    pub clean_session: bool,
    pub keep_alive: u16,
    pub user: Option<String>,
    pub password: Option<String>,
    pub will: Option<Will>,
}

/// One PUBLISH of a client.
#[derive(Clone, Debug)]
pub struct Received {
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: u8,
    pub retain: bool,
    pub dup: bool,
    pub pkid: u16,
}

impl Received {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.payload).into_owned()
    }
}

#[derive(Clone, Default)]
pub struct Behavior {
    /// `Some`: every connection is TLS from the first byte.
    pub tls_config: Option<Arc<ServerConfig>>,
    /// The broker waits this long before it sends the CONNACK.
    pub connack_delay: Duration,
}

impl Behavior {
    pub fn plain() -> Self {
        Self::default()
    }

    pub fn tls(config: Arc<ServerConfig>) -> Self {
        Self {
            tls_config: Some(config),
            ..Self::default()
        }
    }

    pub fn slow(mut self, delay: Duration) -> Self {
        self.connack_delay = delay;
        self
    }
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
    /// The will of each connection that ended with no DISCONNECT.
    fired_wills: Vec<Will>,
    hold_acks: bool,
    /// The broker holds the PUBACK of a QoS 1 message on each of these topics.
    hold_topics: Vec<String>,
    /// The connection number and the packet id of each PUBACK that the broker holds.
    held: Vec<(usize, u16)>,
    live: Vec<(usize, Sender<Out>)>,
    next_connection: usize,
}

type Shared = Arc<Mutex<State>>;

fn lock(state: &Shared) -> MutexGuard<'_, State> {
    state.lock().expect("broker state lock")
}

pub struct MqttBroker {
    pub port: u16,
    state: Shared,
    stop: Arc<AtomicBool>,
    accept_thread: Option<JoinHandle<()>>,
}

/// A free local port. The caller binds it later, so a broker can start after the client.
pub fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .expect("bind a free port")
        .local_addr()
        .expect("local address")
        .port()
}

impl MqttBroker {
    pub fn start(behavior: Behavior) -> Self {
        Self::start_on(0, behavior)
    }

    /// Port 0 picks a free port.
    pub fn start_on(port: u16, behavior: Behavior) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind the MQTT test broker");
        let port = listener.local_addr().expect("local address").port();
        listener
            .set_nonblocking(true)
            .expect("non-blocking listener");
        let state: Shared = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let accept_thread = {
            let state = Arc::clone(&state);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let behavior = behavior.clone();
                            let state = Arc::clone(&state);
                            let stop = Arc::clone(&stop);
                            std::thread::spawn(move || serve(stream, &behavior, &state, &stop));
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
            })
        };
        Self {
            port,
            state,
            stop,
            accept_thread: Some(accept_thread),
        }
    }

    pub fn received(&self) -> Vec<Received> {
        lock(&self.state).received.clone()
    }

    pub fn received_on(&self, topic: &str) -> Vec<Received> {
        self.received()
            .into_iter()
            .filter(|message| message.topic == topic)
            .collect()
    }

    pub fn connects(&self) -> Vec<ConnectInfo> {
        lock(&self.state).connects.clone()
    }

    pub fn subscribes(&self) -> usize {
        lock(&self.state).subscribes
    }

    /// The will of each connection that ended with no DISCONNECT, as a real broker publishes it.
    pub fn fired_wills(&self) -> Vec<Will> {
        lock(&self.state).fired_wills.clone()
    }

    /// While on, the broker keeps each PUBACK of a QoS 1 message until `release_acks`.
    pub fn hold_acks(&self, on: bool) {
        lock(&self.state).hold_acks = on;
    }

    /// Keep the PUBACK of each QoS 1 message on `topic` until `release_acks`. The other messages
    /// get their PUBACK at once.
    pub fn hold_acks_on(&self, topic: &str) {
        lock(&self.state).hold_topics.push(topic.to_owned());
    }

    /// Send every PUBACK that the broker holds, and stop holding the PUBACK of any topic.
    pub fn release_acks(&self) {
        let mut state = lock(&self.state);
        state.hold_topics.clear();
        let held = std::mem::take(&mut state.held);
        for (connection, pkid) in held {
            if let Some((_, sender)) = state.live.iter().find(|(id, _)| *id == connection) {
                let _ = sender.send(Out::Bytes(puback(pkid)));
            }
        }
    }

    /// Send a PUBLISH of QoS 0 to every connection, as a broker does for a subscriber.
    pub fn push_publish(&self, topic: &str, payload: &[u8]) {
        let packet = publish_packet(topic, payload);
        for (_, sender) in &lock(&self.state).live {
            let _ = sender.send(Out::Bytes(packet.clone()));
        }
    }

    /// Close every connection with no DISCONNECT from the client.
    pub fn kick(&self) {
        for (_, sender) in &lock(&self.state).live {
            let _ = sender.send(Out::Close);
        }
    }

    /// Wait up to 10 s until `condition` holds. False when it never holds.
    pub fn wait_until(&self, condition: impl Fn(&Self) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if condition(self) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for MqttBroker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.accept_thread.take() {
            let _ = thread.join();
        }
    }
}

enum Conn {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ServerConnection, TcpStream>>),
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buf),
            Self::Tls(stream) => stream.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.write(buf),
            Self::Tls(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(stream) => stream.flush(),
            Self::Tls(stream) => stream.flush(),
        }
    }
}

/// What one connection knows about itself.
#[derive(Default)]
struct Session {
    will: Option<Will>,
    /// The client sent DISCONNECT.
    clean: bool,
}

fn serve(stream: TcpStream, behavior: &Behavior, state: &Shared, stop: &AtomicBool) {
    let (sender, outbound) = mpsc::channel();
    let id = {
        let mut guard = lock(state);
        let id = guard.next_connection;
        guard.next_connection += 1;
        guard.live.push((id, sender));
        id
    };
    let mut session = Session::default();
    // An I/O error ends the connection: the client closed it or the TLS handshake failed.
    let _ = talk(stream, behavior, state, id, &outbound, stop, &mut session);
    let mut guard = lock(state);
    guard.live.retain(|(live, _)| *live != id);
    guard.held.retain(|(held, _)| *held != id);
    if !session.clean
        && let Some(will) = session.will
    {
        guard.fired_wills.push(will);
    }
}

fn talk(
    stream: TcpStream,
    behavior: &Behavior,
    state: &Shared,
    id: usize,
    outbound: &Receiver<Out>,
    stop: &AtomicBool,
    session: &mut Session,
) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(POLL))?;
    let mut conn = match &behavior.tls_config {
        Some(config) => {
            let connection = ServerConnection::new(Arc::clone(config)).map_err(io::Error::other)?;
            Conn::Tls(Box::new(StreamOwned::new(connection, stream)))
        }
        None => Conn::Plain(stream),
    };
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        while let Ok(out) = outbound.try_recv() {
            match out {
                Out::Bytes(bytes) => {
                    conn.write_all(&bytes)?;
                    conn.flush()?;
                }
                Out::Close => return Ok(()),
            }
        }
        match conn.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(count) => buffer.extend_from_slice(&chunk[..count]),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
        while let Some((first, body)) = take_packet(&mut buffer) {
            if !handle(first, &body, &mut conn, behavior, state, id, session)? {
                return Ok(());
            }
        }
    }
}

/// Take one complete packet from the front of the buffer: the first byte and the body.
fn take_packet(buffer: &mut Vec<u8>) -> Option<(u8, Vec<u8>)> {
    let first = *buffer.first()?;
    let (mut length, mut shift, mut index) = (0usize, 0u32, 1usize);
    loop {
        let byte = *buffer.get(index)?;
        length |= usize::from(byte & 0x7f) << shift;
        index += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift > 21 {
            return None;
        }
    }
    if buffer.len() < index + length {
        return None;
    }
    let body = buffer[index..index + length].to_vec();
    buffer.drain(..index + length);
    Some((first, body))
}

fn take_u8(data: &mut &[u8]) -> Option<u8> {
    let (first, rest) = data.split_first()?;
    *data = rest;
    Some(*first)
}

fn take_u16(data: &mut &[u8]) -> Option<u16> {
    let high = take_u8(data)?;
    let low = take_u8(data)?;
    Some(u16::from_be_bytes([high, low]))
}

fn take_bytes(data: &mut &[u8]) -> Option<Vec<u8>> {
    let length = usize::from(take_u16(data)?);
    if data.len() < length {
        return None;
    }
    let (head, rest) = data.split_at(length);
    *data = rest;
    Some(head.to_vec())
}

fn take_string(data: &mut &[u8]) -> Option<String> {
    String::from_utf8(take_bytes(data)?).ok()
}

fn bad_packet() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "bad MQTT packet")
}

fn parse_connect(body: &[u8]) -> Option<ConnectInfo> {
    let mut data = body;
    let _protocol = take_string(&mut data)?;
    let _level = take_u8(&mut data)?;
    let flags = take_u8(&mut data)?;
    let keep_alive = take_u16(&mut data)?;
    let client_id = take_string(&mut data)?;
    let will = if flags & 0x04 != 0 {
        let topic = take_string(&mut data)?;
        let payload = take_bytes(&mut data)?;
        Some(Will {
            topic,
            payload,
            qos: (flags >> 3) & 3,
            retain: flags & 0x20 != 0,
        })
    } else {
        None
    };
    let user = if flags & 0x80 != 0 {
        Some(take_string(&mut data)?)
    } else {
        None
    };
    let password = if flags & 0x40 != 0 {
        Some(String::from_utf8(take_bytes(&mut data)?).ok()?)
    } else {
        None
    };
    Some(ConnectInfo {
        client_id,
        clean_session: flags & 0x02 != 0,
        keep_alive,
        user,
        password,
        will,
    })
}

fn parse_publish(first: u8, body: &[u8]) -> Option<Received> {
    let mut data = body;
    let qos = (first >> 1) & 3;
    let topic = take_string(&mut data)?;
    let pkid = if qos > 0 { take_u16(&mut data)? } else { 0 };
    Some(Received {
        topic,
        payload: data.to_vec(),
        qos,
        retain: first & 1 != 0,
        dup: first & 8 != 0,
        pkid,
    })
}

fn puback(pkid: u16) -> Vec<u8> {
    let [high, low] = pkid.to_be_bytes();
    vec![0x40, 2, high, low]
}

fn length_bytes(mut length: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let mut byte = (length % 128) as u8;
        length /= 128;
        if length > 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
        if length == 0 {
            return bytes;
        }
    }
}

fn publish_packet(topic: &str, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&(topic.len() as u16).to_be_bytes());
    body.extend_from_slice(topic.as_bytes());
    body.extend_from_slice(payload);
    let mut packet = vec![0x30];
    packet.extend(length_bytes(body.len()));
    packet.extend(body);
    packet
}

/// Handle one packet. False when the connection ends.
fn handle(
    first: u8,
    body: &[u8],
    conn: &mut Conn,
    behavior: &Behavior,
    state: &Shared,
    id: usize,
    session: &mut Session,
) -> io::Result<bool> {
    match first >> 4 {
        1 => {
            let info = parse_connect(body).ok_or_else(bad_packet)?;
            session.will = info.will.clone();
            lock(state).connects.push(info);
            std::thread::sleep(behavior.connack_delay);
            conn.write_all(&[0x20, 2, 0, 0])?;
            conn.flush()?;
        }
        3 => {
            let message = parse_publish(first, body).ok_or_else(bad_packet)?;
            let (qos, pkid) = (message.qos, message.pkid);
            let mut guard = lock(state);
            let topic_held = guard.hold_topics.contains(&message.topic);
            guard.received.push(message);
            if qos == 1 {
                if guard.hold_acks || topic_held {
                    guard.held.push((id, pkid));
                } else {
                    drop(guard);
                    conn.write_all(&puback(pkid))?;
                    conn.flush()?;
                }
            }
        }
        8 => {
            lock(state).subscribes += 1;
            let mut data = body;
            let pkid = take_u16(&mut data).ok_or_else(bad_packet)?;
            let [high, low] = pkid.to_be_bytes();
            conn.write_all(&[0x90, 3, high, low, 0])?;
            conn.flush()?;
        }
        12 => {
            conn.write_all(&[0xd0, 0])?;
            conn.flush()?;
        }
        14 => {
            session.clean = true;
            return Ok(false);
        }
        // The PUBACK of a client for a PUBLISH of the broker, and any other packet.
        _ => {}
    }
    Ok(true)
}
