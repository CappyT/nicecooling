//! Review: two instances of the app for one user write the same files. The app starts at login
//! with `--background`, and the user can start it again from the menu.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use nicecooling::app_settings::write_atomic;
use nicecooling::graphs::store::GraphStore;
use support::graphs::{Dir, constant_graph};

/// Instance A applies `graph_1`, then instance B applies `graph_2`. Both Graphs must be in the
/// file: no Apply of the user is lost.
#[test]
fn two_instances_do_not_lose_an_apply() {
    let dir = Dir::new("rv-two-apply");
    let paths = dir.graph_paths();
    let mut a = GraphStore::open(paths.clone());
    let mut b = GraphStore::open(paths.clone());
    a.put(constant_graph("graph_1", 1.0)).unwrap();
    b.put(constant_graph("graph_2", 2.0)).unwrap();

    let ids: Vec<String> = GraphStore::open(paths)
        .graphs()
        .into_iter()
        .map(|g| g.id)
        .collect();
    assert_eq!(
        ids,
        ["graph_1", "graph_2"],
        "an Apply of instance A is lost"
    );
}

/// Two writers of one file at the same time. A reader must always see one complete content,
/// never a mix or a part, and no write may fail.
#[test]
fn two_writers_never_tear_the_file() {
    let dir = Dir::new("rv-two-writers");
    let path = dir.path().join("graphs.json");
    let a = vec![b'a'; 256 * 1024];
    let b = vec![b'b'; 256 * 1024];
    write_atomic(&path, &a, 0o666).unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let writer = |bytes: Vec<u8>| {
        let path = path.clone();
        let stop = stop.clone();
        std::thread::spawn(move || {
            let mut failures = Vec::new();
            for _ in 0..300 {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                if let Err(error) = write_atomic(&path, &bytes, 0o666) {
                    failures.push(error.to_string());
                }
            }
            failures
        })
    };
    let reader = {
        let path = path.clone();
        let stop = stop.clone();
        std::thread::spawn(move || {
            let mut torn = 0_u32;
            while !stop.load(Ordering::Relaxed) {
                if let Ok(text) = std::fs::read(&path) {
                    let whole = text.len() == 256 * 1024
                        && (text.iter().all(|&c| c == b'a') || text.iter().all(|&c| c == b'b'));
                    if !whole {
                        torn += 1;
                    }
                }
                std::thread::sleep(Duration::from_micros(200));
            }
            torn
        })
    };
    let first = writer(a);
    let second = writer(b);
    let mut failures = first.join().unwrap();
    failures.extend(second.join().unwrap());
    stop.store(true, Ordering::Relaxed);
    let torn = reader.join().unwrap();
    failures.sort();
    failures.dedup();
    assert!(
        torn == 0 && failures.is_empty(),
        "{torn} torn reads, write failures: {failures:?}"
    );
}
