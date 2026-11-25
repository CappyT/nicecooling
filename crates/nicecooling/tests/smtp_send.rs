mod support;

use std::time::{Duration, Instant};

use nicecooling::graphs::secrets::Password;
use nicecooling::smtp::send::{Mail, SEND_LIMIT, SendError, SmtpAccount, send};
use nicecooling::smtp::settings::{SmtpSecurity, SmtpSettings};
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

fn mail() -> Mail {
    Mail {
        to: "owner@example.org".to_owned(),
        subject: "Pump stopped".to_owned(),
        text: "The pump stopped.".to_owned(),
    }
}

#[tokio::test]
async fn smtp_send_ok() {
    let pki = TestPki::new();
    let server = SmtpServer::start(Behavior::starttls(pki.server_config.clone()));
    let account = SmtpAccount::new(
        settings(server.port, SmtpSecurity::Starttls),
        Password::default(),
    )
    .with_extra_root(pki.ca_der.clone());

    assert_eq!(send(&account, &mail()).await, Ok(()));

    let sessions = server.sessions();
    assert_eq!(sessions.len(), 1);
    let session = &sessions[0];
    assert!(session.starttls);
    assert_eq!(session.mail_encrypted, Some(true));
    let data = session.data.as_deref().expect("the server got a message");
    assert!(data.contains("The pump stopped."), "{data}");
    assert!(data.contains("Content-Type: text/plain"), "{data}");
}

#[tokio::test]
async fn smtp_tls_wrapper_send_ok() {
    let pki = TestPki::new();
    let server = SmtpServer::start(Behavior::wrapper(pki.server_config.clone()));
    let account = SmtpAccount::new(
        settings(server.port, SmtpSecurity::Tls),
        Password::default(),
    )
    .with_extra_root(pki.ca_der.clone());

    assert_eq!(send(&account, &mail()).await, Ok(()));

    let sessions = server.sessions();
    assert_eq!(sessions.len(), 1);
    assert!(!sessions[0].starttls);
    assert_eq!(sessions[0].mail_encrypted, Some(true));
}

#[tokio::test]
async fn smtp_starttls_with_an_invalid_certificate_fails() {
    let pki = TestPki::new();
    let server = SmtpServer::start(Behavior::starttls(pki.server_config.clone()));
    // The test CA is not in the system certificates, and this account adds no root.
    let account = SmtpAccount::new(
        settings(server.port, SmtpSecurity::Starttls),
        Password::default(),
    );

    let result = send(&account, &mail()).await;

    assert!(matches!(result, Err(SendError::Failed(_))), "{result:?}");
    // The client reached the TLS step, and nothing followed it.
    assert!(server.wait_for(|sessions| sessions.iter().any(|session| session.starttls)));
    assert!(
        server
            .sessions()
            .iter()
            .all(|session| session.mail_from.is_none() && session.data.is_none())
    );
}

#[tokio::test]
async fn smtp_none_logs_in() {
    let server = SmtpServer::start(Behavior::plain());
    let mut login = settings(server.port, SmtpSecurity::None);
    login.user = "bob".to_owned();
    assert!(login.warns_plain_login());
    let account = SmtpAccount::new(login, Password::new("hunter2"));

    assert_eq!(send(&account, &mail()).await, Ok(()));

    let sessions = server.sessions();
    assert_eq!(sessions.len(), 1);
    // `AUTH PLAIN` carries the base64 text of NUL, the user name, NUL and the password.
    assert_eq!(sessions[0].auth_lines, ["AUTH PLAIN AGJvYgBodW50ZXIy"]);
    assert_eq!(sessions[0].auth_encrypted, Some(false));
    assert!(sessions[0].data.is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smtp_slow_server_times_out_at_30s_and_evaluation_stays_on_time() {
    let account = SmtpAccount::new(settings(1, SmtpSecurity::None), Password::default());
    assert_eq!(SEND_LIMIT, Duration::from_secs(30));
    assert_eq!(account.limit(), SEND_LIMIT);

    // The server waits 5 s before its greeting. The test shortens the limit to 300 ms.
    let server = SmtpServer::start(Behavior::plain().slow(Duration::from_secs(5)));
    let account = SmtpAccount::new(
        settings(server.port, SmtpSecurity::None),
        Password::default(),
    )
    .with_limit(Duration::from_millis(300));
    let started = Instant::now();
    let sending = tokio::spawn(async move { send(&account, &mail()).await });

    // The evaluation of the Graph service ticks while the send waits.
    let mut longest_gap = Duration::ZERO;
    let mut last = Instant::now();
    while !sending.is_finished() {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let now = Instant::now();
        longest_gap = longest_gap.max(now - last);
        last = now;
    }

    let result = sending.await.expect("the send task ends");
    assert_eq!(result, Err(SendError::Timeout));
    assert!(started.elapsed() >= Duration::from_millis(300));
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(longest_gap < Duration::from_millis(250), "{longest_gap:?}");
}

#[tokio::test]
async fn smtp_subject_line_break_is_stripped() {
    let server = SmtpServer::start(Behavior::plain());
    let account = SmtpAccount::new(
        settings(server.port, SmtpSecurity::None),
        Password::default(),
    );
    let mail = Mail {
        subject: "Pump stopped\r\nBcc: evil@example.org".to_owned(),
        ..mail()
    };

    assert_eq!(send(&account, &mail).await, Ok(()));

    let sessions = server.sessions();
    let lines = sessions[0].data_lines();
    let headers: Vec<&String> = lines.iter().take_while(|line| !line.is_empty()).collect();
    let subjects: Vec<&&String> = headers
        .iter()
        .filter(|line| line.starts_with("Subject:"))
        .collect();
    assert_eq!(subjects.len(), 1, "{headers:?}");
    assert_eq!(
        subjects[0].as_str(),
        "Subject: Pump stoppedBcc: evil@example.org"
    );
    assert!(headers.iter().all(|line| !line.starts_with("Bcc:")));
    assert_eq!(sessions[0].rcpt_to, ["owner@example.org"]);
}

#[tokio::test]
async fn smtp_envelope_has_recipient_and_sender() {
    let server = SmtpServer::start(Behavior::plain());
    let account = SmtpAccount::new(
        settings(server.port, SmtpSecurity::None),
        Password::default(),
    );

    assert_eq!(send(&account, &mail()).await, Ok(()));

    let sessions = server.sessions();
    // The envelope holds the bare addresses. The From header keeps the display name.
    assert_eq!(
        sessions[0].mail_from.as_deref(),
        Some("nicecooling@example.org")
    );
    assert_eq!(sessions[0].rcpt_to, ["owner@example.org"]);
    let lines = sessions[0].data_lines();
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("From: ") && line.contains("nicecooling@example.org")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("To: ") && line.contains("owner@example.org")),
        "{lines:?}"
    );
}

#[tokio::test]
async fn smtp_without_an_account_is_not_configured() {
    let account = SmtpAccount::new(SmtpSettings::default(), Password::default());
    assert_eq!(send(&account, &mail()).await, Err(SendError::NotConfigured));
}
