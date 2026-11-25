//! One instance of the app for each session bus. Each test has a private bus.

mod support;

use std::time::{Duration, Instant};

use nicecooling::command::Command;
use nicecooling::instance::{Claim, claim, show_first};
use support::bus::PrivateBus;
use support::{SESSION_LOCK, use_as_session_bus};
use tokio::sync::mpsc;

#[tokio::test(flavor = "multi_thread")]
async fn the_second_claim_finds_the_name_taken() {
    let bus = PrivateBus::start("instance-taken");
    let (tx, _rx) = mpsc::unbounded_channel();
    let first = claim(Some(&bus.address), tx.clone()).await;
    assert!(matches!(first, Claim::First(_)), "{first:?}");
    let second = claim(Some(&bus.address), tx.clone()).await;
    assert!(matches!(second, Claim::Taken), "{second:?}");
    // The failed claim did not take the name from the first instance.
    let third = claim(Some(&bus.address), tx).await;
    assert!(matches!(third, Claim::Taken), "{third:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_second_start_shows_the_window_of_the_first() {
    let bus = PrivateBus::start("instance-show");
    let (tx, mut rx) = mpsc::unbounded_channel();
    let first = claim(Some(&bus.address), tx).await;
    assert!(matches!(first, Claim::First(_)), "{first:?}");

    show_first(Some(&bus.address)).await.unwrap();
    let command = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("a command in 5 s");
    assert_eq!(command, Some(Command::OpenWindow(None)));
}

#[tokio::test(flavor = "multi_thread")]
async fn show_without_a_first_instance_is_an_error() {
    let bus = PrivateBus::start("instance-none");
    assert!(show_first(Some(&bus.address)).await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_bus_the_app_starts_as_before() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let result = claim(Some("unix:path=/nonexistent/nicecooling-test-bus"), tx).await;
    assert!(matches!(result, Claim::NoBus(_)), "{result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_name_is_free_when_the_first_instance_ends() {
    let bus = PrivateBus::start("instance-ends");
    let (tx, _rx) = mpsc::unbounded_channel();
    let first = claim(Some(&bus.address), tx.clone()).await;
    assert!(matches!(first, Claim::First(_)), "{first:?}");
    drop(first);

    // The bus sees the end of the connection a moment later.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let again = claim(Some(&bus.address), tx.clone()).await;
        if matches!(again, Claim::First(_)) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the name stayed taken: {again:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// `None` is the session bus of the user: the address comes from `DBUS_SESSION_BUS_ADDRESS`.
#[tokio::test(flavor = "multi_thread")]
async fn the_session_bus_of_the_environment_is_the_default() {
    let _guard = SESSION_LOCK.lock().await;
    let bus = PrivateBus::start("instance-session");
    use_as_session_bus(&bus);
    let (tx, mut rx) = mpsc::unbounded_channel();
    let first = claim(None, tx.clone()).await;
    assert!(matches!(first, Claim::First(_)), "{first:?}");
    let second = claim(None, tx).await;
    assert!(matches!(second, Claim::Taken), "{second:?}");
    show_first(None).await.unwrap();
    assert_eq!(rx.recv().await, Some(Command::OpenWindow(None)));
}
