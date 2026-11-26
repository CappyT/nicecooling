//! Review 2, the SMTP sender of the app. Test SMTP server in this process only.

mod support;

use std::sync::Arc;

use nicecooling::graphs::secrets::Password;
use nicecooling::smtp::send::{Mail, SendError, SmtpAccount, send};
use nicecooling::smtp::settings::{SmtpSecurity, SmtpSettings};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use rustls::ServerConfig;
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use support::smtp_server::{Behavior, SmtpServer, TestPki};

const SENDER: &str = "NiceCooling <nicecooling@example.org>";

fn settings(port: u16, security: SmtpSecurity) -> SmtpSettings {
    SmtpSettings {
        host: "127.0.0.1".to_owned(),
        port: Some(port),
        security,
        user: String::new(),
        sender: SENDER.to_owned(),
    }
}

fn mail(to: &str, subject: &str, text: &str) -> Mail {
    Mail {
        to: to.to_owned(),
        subject: subject.to_owned(),
        text: text.to_owned(),
    }
}

fn plain_account(server: &SmtpServer) -> SmtpAccount {
    SmtpAccount::new(
        settings(server.port, SmtpSecurity::None),
        Password::default(),
    )
}

fn wrong_name_pki() -> (Vec<u8>, Arc<ServerConfig>) {
    let ca_key = KeyPair::generate().unwrap();
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "Review CA");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign];
    let ca_cert = ca_params.self_signed(&ca_key).unwrap();
    let issuer = Issuer::new(ca_params, ca_key);
    let leaf_key = KeyPair::generate().unwrap();
    let mut leaf_params = CertificateParams::new(vec!["wrong.example".to_owned()]).unwrap();
    leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let leaf = leaf_params.signed_by(&leaf_key, &issuer).unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![leaf.der().clone()],
            PrivateKeyDer::from(PrivatePkcs8KeyDer::from(leaf_key.serialize_der())),
        )
        .unwrap();
    (ca_cert.der().to_vec(), Arc::new(config))
}

/// A recipient with a line break and a second header is not an address.
#[tokio::test]
async fn a_recipient_with_a_line_break_sends_nothing() {
    let server = SmtpServer::start(Behavior::plain());
    let to = "owner@example.org\r\nBcc: thief@example.net";
    let result = send(&plain_account(&server), &mail(to, "Hot", "Text")).await;
    assert!(matches!(result, Err(SendError::Failed(_))), "{result:?}");
    for session in server.sessions() {
        assert!(
            session.rcpt_to.iter().all(|r| !r.contains("thief")),
            "{session:?}"
        );
        assert!(session.data.is_none());
    }
}

/// One recipient address: a list of two is refused.
#[tokio::test]
async fn two_recipients_send_nothing() {
    let server = SmtpServer::start(Behavior::plain());
    let to = "owner@example.org, thief@example.net";
    let result = send(&plain_account(&server), &mail(to, "Hot", "Text")).await;
    assert!(matches!(result, Err(SendError::Failed(_))), "{result:?}");
    assert!(server.sessions().iter().all(|s| s.rcpt_to.is_empty()));
}

/// A sender with a line break is not an address.
#[tokio::test]
async fn a_sender_with_a_line_break_sends_nothing() {
    let server = SmtpServer::start(Behavior::plain());
    let mut account_settings = settings(server.port, SmtpSecurity::None);
    account_settings.sender = "NiceCooling <a@example.org>\r\nBcc: thief@example.net".to_owned();
    let account = SmtpAccount::new(account_settings, Password::default());
    let result = send(&account, &mail("owner@example.org", "Hot", "Text")).await;
    assert!(matches!(result, Err(SendError::Failed(_))), "{result:?}");
    assert!(server.sessions().iter().all(|s| s.data.is_none()));
}

/// A placeholder value with CR, LF and a header in the subject stays in the one subject line.
#[tokio::test]
async fn a_subject_with_a_header_stays_one_header() {
    let server = SmtpServer::start(Behavior::plain());
    let subject = "Hot\r\nBcc: thief@example.net\rX: y\nZ: z";
    assert_eq!(
        send(
            &plain_account(&server),
            &mail("owner@example.org", subject, "Text")
        )
        .await,
        Ok(())
    );
    let sessions = server.sessions();
    let data = sessions[0].data.as_deref().unwrap();
    let headers = data.split("\r\n\r\n").next().unwrap();
    assert!(
        headers
            .lines()
            .all(|l| !l.starts_with("Bcc:") && !l.starts_with("X:") && !l.starts_with("Z:")),
        "{headers}"
    );
    assert_eq!(sessions[0].rcpt_to, ["owner@example.org"]);
}

/// The mode `starttls` against a server with no `STARTTLS` fails and never sends in clear text.
#[tokio::test]
async fn starttls_without_starttls_on_the_server_fails_in_clear_text_never() {
    let server = SmtpServer::start(Behavior::plain());
    let mut login = settings(server.port, SmtpSecurity::Starttls);
    login.user = "bob".to_owned();
    let account = SmtpAccount::new(login, Password::new("hunter2-secret"));
    let result = send(&account, &mail("owner@example.org", "Hot", "Text")).await;
    assert!(matches!(result, Err(SendError::Failed(_))), "{result:?}");
    if let Err(SendError::Failed(reason)) = &result {
        assert!(!reason.contains("hunter2"), "{reason}");
    }
    for session in server.sessions() {
        assert!(session.auth_lines.is_empty(), "{session:?}");
        assert!(
            session.mail_from.is_none() && session.data.is_none(),
            "{session:?}"
        );
    }
}

/// The mode `tls` with a certificate that no trusted CA signed fails before any command.
#[tokio::test]
async fn tls_wrapper_with_an_untrusted_certificate_fails() {
    let pki = TestPki::new();
    let server = SmtpServer::start(Behavior::wrapper(pki.server_config.clone()));
    let account = SmtpAccount::new(
        settings(server.port, SmtpSecurity::Tls),
        Password::default(),
    );
    let result = send(&account, &mail("owner@example.org", "Hot", "Text")).await;
    assert!(matches!(result, Err(SendError::Failed(_))), "{result:?}");
    assert!(server.sessions().iter().all(|s| s.mail_from.is_none()));
}

/// A trusted CA that signed a certificate for another host name fails: the name is checked.
#[tokio::test]
async fn a_certificate_for_another_host_name_fails() {
    let (ca_der, config) = wrong_name_pki();
    let server = SmtpServer::start(Behavior::wrapper(config));
    let account = SmtpAccount::new(
        settings(server.port, SmtpSecurity::Tls),
        Password::default(),
    )
    .with_extra_root(ca_der);
    let result = send(&account, &mail("owner@example.org", "Hot", "Text")).await;
    assert!(matches!(result, Err(SendError::Failed(_))), "{result:?}");
    assert!(server.sessions().iter().all(|s| s.mail_from.is_none()));
}

/// A line that is only a dot ends the data on the wire unless the client doubles it.
#[tokio::test]
async fn a_body_line_that_is_only_a_dot_is_sent_whole() {
    let server = SmtpServer::start(Behavior::plain());
    let text = "first\r\n.\r\nlast line";
    assert_eq!(
        send(
            &plain_account(&server),
            &mail("owner@example.org", "Hot", text)
        )
        .await,
        Ok(())
    );
    let data = server.sessions()[0].data.clone().unwrap();
    assert!(data.contains("last line"), "{data}");
}

/// Non-ASCII text in the subject goes as an encoded word, never as raw bytes in a header.
#[tokio::test]
async fn non_ascii_subject_and_body_are_encoded() {
    let server = SmtpServer::start(Behavior::plain());
    let subject = "Wasser 46,2 \u{b0}C \u{2713}";
    let text = "Temperatur \u{fc}ber 46,2 \u{b0}C";
    assert_eq!(
        send(
            &plain_account(&server),
            &mail("owner@example.org", subject, text)
        )
        .await,
        Ok(())
    );
    let data = server.sessions()[0].data.clone().unwrap();
    let headers = data.split("\r\n\r\n").next().unwrap();
    assert!(headers.is_ascii(), "{headers}");
    assert!(headers.contains("=?utf-8?b?"), "{headers}");
}
