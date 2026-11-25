//! The SMTP sender. It sends one plain text message and gives one outcome. It holds no global
//! state: the caller runs it in its own task, so a slow server never delays the evaluation.

use std::time::Duration;

use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Certificate, Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use super::settings::{SmtpSecurity, SmtpSettings};
use crate::graphs::secrets::Password;

/// The time limit of one send: the connection, the login and the transfer together.
pub const SEND_LIMIT: Duration = Duration::from_secs(30);

/// One message of an e-mail response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mail {
    pub to: String,
    pub subject: String,
    pub text: String,
}

/// Why a send gives no `ok`. The reason text holds no password.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SendError {
    /// The account has no host or no sender address.
    NotConfigured,
    /// The server did not finish the send in the time limit.
    Timeout,
    /// The send failed: an address that does not parse, a refused certificate, a refusal of the
    /// server, a network error.
    Failed(String),
}

/// The account of one send.
#[derive(Clone, Debug)]
pub struct SmtpAccount {
    pub settings: SmtpSettings,
    pub password: Password,
    extra_roots: Vec<Vec<u8>>,
    limit: Duration,
}

impl SmtpAccount {
    pub fn new(settings: SmtpSettings, password: Password) -> Self {
        Self {
            settings,
            password,
            extra_roots: Vec::new(),
            limit: SEND_LIMIT,
        }
    }

    /// Trust one more root certificate (DER) next to the system certificates. This only adds a
    /// trust anchor. The sender has no way to skip the check of the certificate.
    pub fn with_extra_root(mut self, der: Vec<u8>) -> Self {
        self.extra_roots.push(der);
        self
    }

    /// A shorter limit than `SEND_LIMIT`, so a test does not wait 30 s.
    pub fn with_limit(mut self, limit: Duration) -> Self {
        self.limit = limit;
        self
    }

    pub fn limit(&self) -> Duration {
        self.limit
    }
}

/// A subject is one header line. CR and LF would start another header.
fn on_one_line(subject: &str) -> String {
    subject
        .chars()
        .filter(|c| !matches!(c, '\r' | '\n'))
        .collect()
}

fn failed(reason: impl std::fmt::Display) -> SendError {
    SendError::Failed(reason.to_string())
}

fn build_message(settings: &SmtpSettings, mail: &Mail) -> Result<Message, SendError> {
    let from: Mailbox = settings
        .sender
        .trim()
        .parse()
        .map_err(|_| failed("the sender address is not valid"))?;
    let to: Mailbox = mail
        .to
        .trim()
        .parse()
        .map_err(|_| failed("the recipient address is not valid"))?;
    Message::builder()
        .from(from)
        .to(to)
        .subject(on_one_line(&mail.subject))
        .header(ContentType::TEXT_PLAIN)
        .body(mail.text.clone())
        .map_err(failed)
}

/// The TLS parameters check the certificate of the server against the system certificates and
/// the extra roots, and the host name against the certificate.
fn tls_parameters(account: &SmtpAccount) -> Result<TlsParameters, SendError> {
    let mut builder = TlsParameters::builder(account.settings.host.trim().to_owned());
    for der in &account.extra_roots {
        builder = builder.add_root_certificate(Certificate::from_der(der.clone()).map_err(failed)?);
    }
    builder.build().map_err(failed)
}

fn build_transport(account: &SmtpAccount) -> Result<AsyncSmtpTransport<Tokio1Executor>, SendError> {
    let settings = &account.settings;
    let tls = match settings.security {
        SmtpSecurity::Starttls => Tls::Required(tls_parameters(account)?),
        SmtpSecurity::Tls => Tls::Wrapper(tls_parameters(account)?),
        SmtpSecurity::None => Tls::None,
    };
    let mut builder = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(settings.host.trim())
        .port(settings.effective_port())
        .tls(tls);
    if settings.logs_in() {
        builder = builder.credentials(Credentials::new(
            settings.user.clone(),
            account.password.expose().to_owned(),
        ));
    }
    Ok(builder.build())
}

/// Send one message. The call ends in the time limit of the account at the latest.
pub async fn send(account: &SmtpAccount, mail: &Mail) -> Result<(), SendError> {
    if !account.settings.is_configured() {
        return Err(SendError::NotConfigured);
    }
    let message = build_message(&account.settings, mail)?;
    let transport = build_transport(account)?;
    match tokio::time::timeout(account.limit, transport.send(message)).await {
        Err(_elapsed) => Err(SendError::Timeout),
        Ok(Ok(_response)) => Ok(()),
        Ok(Err(error)) if error.is_timeout() => Err(SendError::Timeout),
        Ok(Err(error)) => Err(failed(error)),
    }
}
