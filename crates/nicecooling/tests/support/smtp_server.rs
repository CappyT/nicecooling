//! A small SMTP server for the tests of the sender. It listens on a local port and keeps what
//! each session sends. It speaks the part of RFC 5321 that the sender uses: `EHLO`, `STARTTLS`,
//! `AUTH PLAIN`, `MAIL`, `RCPT`, `DATA` and `QUIT`. The TLS acceptor uses a certificate that
//! `rcgen` makes for `127.0.0.1` and `localhost`, signed by a test CA that is not in the system
//! store. The server uses threads and blocking sockets and needs no runtime.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

/// A test CA and a server certificate that the CA signed.
pub struct TestPki {
    /// The DER of the CA certificate. A client adds it as an extra root.
    pub ca_der: Vec<u8>,
    /// The same certificate in PEM form, for a client that reads a CA file.
    pub ca_pem: String,
    pub server_config: Arc<ServerConfig>,
}

impl TestPki {
    pub fn new() -> Self {
        let ca_key = KeyPair::generate().expect("CA key");
        let mut ca_params = CertificateParams::new(Vec::<String>::new()).expect("CA parameters");
        ca_params
            .distinguished_name
            .push(DnType::CommonName, "NiceCooling test CA");
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign];
        let ca_cert = ca_params.self_signed(&ca_key).expect("CA certificate");
        let issuer = Issuer::new(ca_params, ca_key);

        let leaf_key = KeyPair::generate().expect("server key");
        let mut leaf_params =
            CertificateParams::new(vec!["127.0.0.1".to_owned(), "localhost".to_owned()])
                .expect("server parameters");
        leaf_params
            .distinguished_name
            .push(DnType::CommonName, "NiceCooling test server");
        leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let leaf_cert = leaf_params
            .signed_by(&leaf_key, &issuer)
            .expect("server certificate");

        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let server_config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("TLS versions")
            .with_no_client_auth()
            .with_single_cert(
                vec![leaf_cert.der().clone()],
                PrivateKeyDer::from(PrivatePkcs8KeyDer::from(leaf_key.serialize_der())),
            )
            .expect("server TLS configuration");
        Self {
            ca_der: ca_cert.der().to_vec(),
            ca_pem: ca_cert.pem(),
            server_config: Arc::new(server_config),
        }
    }
}

/// How the server offers TLS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsMode {
    /// No TLS. The server does not offer `STARTTLS`.
    Plain,
    /// The server offers `STARTTLS` on the plain connection.
    Starttls,
    /// The connection is TLS from the first byte.
    Wrapper,
}

#[derive(Clone)]
pub struct Behavior {
    pub tls: TlsMode,
    pub tls_config: Option<Arc<ServerConfig>>,
    /// The server waits this long before it sends the greeting.
    pub greeting_delay: Duration,
    /// The server offers `AUTH PLAIN` on a plain connection and after TLS.
    pub offer_auth: bool,
}

impl Behavior {
    pub fn plain() -> Self {
        Self {
            tls: TlsMode::Plain,
            tls_config: None,
            greeting_delay: Duration::ZERO,
            offer_auth: true,
        }
    }

    pub fn starttls(config: Arc<ServerConfig>) -> Self {
        Self {
            tls: TlsMode::Starttls,
            tls_config: Some(config),
            ..Self::plain()
        }
    }

    pub fn wrapper(config: Arc<ServerConfig>) -> Self {
        Self {
            tls: TlsMode::Wrapper,
            tls_config: Some(config),
            ..Self::plain()
        }
    }

    pub fn slow(mut self, delay: Duration) -> Self {
        self.greeting_delay = delay;
        self
    }
}

/// What one connection sent.
#[derive(Clone, Debug, Default)]
pub struct Session {
    /// The client sent `STARTTLS` and the server answered.
    pub starttls: bool,
    /// The `AUTH` lines as the client sent them, for example `AUTH PLAIN AGJvYgBodW50ZXIy`.
    pub auth_lines: Vec<String>,
    /// Whether the connection was encrypted at the first `AUTH`.
    pub auth_encrypted: Option<bool>,
    /// The address of `MAIL FROM`, without the angle brackets.
    pub mail_from: Option<String>,
    /// Whether the connection was encrypted at `MAIL`.
    pub mail_encrypted: Option<bool>,
    pub rcpt_to: Vec<String>,
    /// The message of `DATA` with the dot stuffing removed and CRLF line ends.
    pub data: Option<String>,
}

impl Session {
    /// The lines of the message, for the headers and the body.
    pub fn data_lines(&self) -> Vec<String> {
        self.data
            .as_deref()
            .unwrap_or_default()
            .split("\r\n")
            .map(str::to_owned)
            .collect()
    }
}

type Sessions = Arc<Mutex<Vec<Session>>>;

pub struct SmtpServer {
    pub port: u16,
    sessions: Sessions,
    stop: Arc<AtomicBool>,
    accept_thread: Option<JoinHandle<()>>,
}

impl SmtpServer {
    pub fn start(behavior: Behavior) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind the SMTP test server");
        let port = listener.local_addr().expect("local address").port();
        listener
            .set_nonblocking(true)
            .expect("non-blocking listener");
        let sessions: Sessions = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let accept_thread = {
            let sessions = Arc::clone(&sessions);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let behavior = behavior.clone();
                            let sessions = Arc::clone(&sessions);
                            let stop = Arc::clone(&stop);
                            std::thread::spawn(move || serve(stream, &behavior, &sessions, &stop));
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
            sessions,
            stop,
            accept_thread: Some(accept_thread),
        }
    }

    pub fn sessions(&self) -> Vec<Session> {
        self.sessions.lock().expect("sessions lock").clone()
    }

    /// Wait up to 5 s until `condition` holds for the sessions. False when it never holds.
    pub fn wait_for(&self, condition: impl Fn(&[Session]) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if condition(self.sessions().as_slice()) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for SmtpServer {
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

impl Conn {
    fn encrypted(&self) -> bool {
        matches!(self, Self::Tls(_))
    }
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

fn tls_conn(config: &Arc<ServerConfig>, stream: TcpStream) -> io::Result<Conn> {
    let connection = ServerConnection::new(Arc::clone(config)).map_err(io::Error::other)?;
    Ok(Conn::Tls(Box::new(StreamOwned::new(connection, stream))))
}

fn update(sessions: &Sessions, index: usize, change: impl FnOnce(&mut Session)) {
    change(&mut sessions.lock().expect("sessions lock")[index]);
}

fn pause(delay: Duration, stop: &AtomicBool) {
    let deadline = Instant::now() + delay;
    while Instant::now() < deadline && !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn say(reader: &mut BufReader<Conn>, text: &str) -> io::Result<()> {
    let conn = reader.get_mut();
    conn.write_all(text.as_bytes())?;
    conn.flush()
}

/// The text between the angle brackets of `MAIL FROM:<a@b>` or `RCPT TO:<a@b>`.
fn address_of(command: &str) -> String {
    command
        .split_once('<')
        .and_then(|(_, rest)| rest.split_once('>'))
        .map(|(address, _)| address.to_owned())
        .unwrap_or_default()
}

/// One connection. An I/O error ends it: the client closed or the TLS handshake failed.
fn serve(stream: TcpStream, behavior: &Behavior, sessions: &Sessions, stop: &AtomicBool) {
    let index = {
        let mut all = sessions.lock().expect("sessions lock");
        all.push(Session::default());
        all.len() - 1
    };
    let _ = run(stream, behavior, sessions, index, stop);
}

fn run(
    stream: TcpStream,
    behavior: &Behavior,
    sessions: &Sessions,
    index: usize,
    stop: &AtomicBool,
) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let tls_config = behavior.tls_config.as_ref();
    let conn = match (behavior.tls, tls_config) {
        (TlsMode::Wrapper, Some(config)) => tls_conn(config, stream)?,
        _ => Conn::Plain(stream),
    };
    pause(behavior.greeting_delay, stop);
    let mut reader = BufReader::new(conn);
    say(&mut reader, "220 localhost ESMTP test server\r\n")?;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let command = line.trim_end_matches(['\r', '\n']).to_owned();
        let upper = command.to_ascii_uppercase();
        let encrypted = reader.get_ref().encrypted();
        if upper.starts_with("EHLO") || upper.starts_with("HELO") {
            let mut reply = String::from("250-localhost\r\n");
            if behavior.tls == TlsMode::Starttls && !encrypted {
                reply.push_str("250-STARTTLS\r\n");
            }
            if behavior.offer_auth && (encrypted || behavior.tls == TlsMode::Plain) {
                reply.push_str("250-AUTH PLAIN\r\n");
            }
            reply.push_str("250 8BITMIME\r\n");
            say(&mut reader, &reply)?;
        } else if upper == "STARTTLS" && behavior.tls == TlsMode::Starttls && !encrypted {
            let Some(config) = tls_config else {
                return Ok(());
            };
            say(&mut reader, "220 ready to start TLS\r\n")?;
            update(sessions, index, |session| session.starttls = true);
            // The client sends nothing before it starts the handshake, so the buffer is empty.
            let Conn::Plain(stream) = reader.into_inner() else {
                return Ok(());
            };
            reader = BufReader::new(tls_conn(config, stream)?);
        } else if upper.starts_with("AUTH PLAIN") {
            update(sessions, index, |session| {
                session.auth_lines.push(command.clone());
                session.auth_encrypted.get_or_insert(encrypted);
            });
            say(&mut reader, "235 2.7.0 authentication successful\r\n")?;
        } else if upper.starts_with("MAIL FROM:") {
            update(sessions, index, |session| {
                session.mail_from = Some(address_of(&command));
                session.mail_encrypted = Some(encrypted);
            });
            say(&mut reader, "250 2.1.0 ok\r\n")?;
        } else if upper.starts_with("RCPT TO:") {
            update(sessions, index, |session| {
                session.rcpt_to.push(address_of(&command));
            });
            say(&mut reader, "250 2.1.5 ok\r\n")?;
        } else if upper == "DATA" {
            say(&mut reader, "354 end data with <CR><LF>.<CR><LF>\r\n")?;
            let mut data = String::new();
            loop {
                let mut part = String::new();
                if reader.read_line(&mut part)? == 0 {
                    return Ok(());
                }
                if part == ".\r\n" {
                    break;
                }
                // A line that starts with a dot has a second dot in front of it on the wire.
                data.push_str(if part.starts_with("..") {
                    &part[1..]
                } else {
                    part.as_str()
                });
            }
            // The data goes in the session before the reply, so a client that sees `250` finds it.
            update(sessions, index, |session| {
                session.data = Some(data.trim_end_matches("\r\n").to_owned());
            });
            say(&mut reader, "250 2.0.0 queued\r\n")?;
        } else if upper == "QUIT" {
            say(&mut reader, "221 2.0.0 bye\r\n")?;
            return Ok(());
        } else if upper == "NOOP" || upper == "RSET" {
            say(&mut reader, "250 2.0.0 ok\r\n")?;
        } else {
            say(&mut reader, "502 5.5.1 command not implemented\r\n")?;
        }
    }
}
