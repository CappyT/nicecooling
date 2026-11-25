//! Runs each fenced `busctl` block of `docs/api.md` against the test daemon on a private bus.

mod support;

use std::path::PathBuf;
use std::process::Command;

use support::Rig;

/// The text of each block that is fenced as `busctl`.
fn blocks(markdown: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    for line in markdown.lines() {
        match (&mut current, line.trim_end()) {
            (None, "```busctl") => current = Some(String::new()),
            (Some(_), "```") => blocks.extend(current.take()),
            (Some(block), text) => {
                block.push_str(text);
                block.push('\n');
            }
            (None, _) => {}
        }
    }
    blocks
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_busctl_block_exits_0() {
    let docs = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/api.md");
    let blocks = blocks(&std::fs::read_to_string(docs).unwrap());
    assert!(blocks.len() >= 13, "{} busctl blocks", blocks.len());

    let mut rig = Rig::start().await;
    rig.ticks(2).await;
    rig.tick_in_background();
    let address = rig.bus.address.clone();
    for block in blocks {
        // The block runs with `busctl` pointed at the private bus.
        let script = format!(
            "set -e\nbusctl() {{ command busctl --address=\"$BUS_ADDRESS\" --no-pager \"$@\"; }}\n{block}"
        );
        let address = address.clone();
        let shown = block.clone();
        let output = tokio::task::spawn_blocking(move || {
            Command::new("bash")
                .args(["-c", &script])
                .env("BUS_ADDRESS", address)
                .output()
                .expect("bash runs")
        })
        .await
        .unwrap();
        assert!(
            output.status.success(),
            "block failed:\n{shown}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
