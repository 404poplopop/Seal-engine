//! `seal-run graph run` gives graphs the players named on the command line.

use std::process::Command;

#[test]
fn graph_run_hands_out_the_players_it_is_given() {
    let graph = concat!(env!("CARGO_MANIFEST_DIR"), "/../seal-graph-tests/graphs/players.graph");
    let out = Command::new(env!("CARGO_BIN_EXE_seal-run"))
        .args(["graph", "run", graph, "--player", "7", "--player", "3"])
        .args(["--fire", "who:0", "--fire", "who:1", "--fire", "who:2"])
        .output()
        .expect("seal-run starts");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}{}", String::from_utf8_lossy(&out.stderr));
    let logs: Vec<&str> = text.lines().filter_map(|l| l.strip_prefix("log: ")).collect();
    assert_eq!(logs, ["true7v0", "true3v0", "falsenone"]);
}

fn hash(text: &str) -> &str {
    text.lines().find_map(|l| l.split("state hash ").nth(1)).expect("a state hash line")
}

#[test]
fn a_recorded_run_replays_to_the_same_state() {
    let graph = concat!(env!("CARGO_MANIFEST_DIR"), "/../seal-graph-tests/graphs/players.graph");
    let rec = std::env::temp_dir().join(format!("seal-run-replay-{}.rec", std::process::id()));
    let rec = rec.to_str().unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_seal-run"))
        .args(["graph", "run", graph, "--player", "7", "--player", "3"])
        .args(["--fire", "who:0", "--fire", "who:1", "--frames", "3", "--record", rec])
        .output()
        .expect("seal-run starts");
    let ran = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(run.status.success(), "{ran}{}", String::from_utf8_lossy(&run.stderr));
    let back = Command::new(env!("CARGO_BIN_EXE_seal-run"))
        .args(["graph", "replay", graph, rec])
        .output()
        .expect("seal-run starts");
    let _ = std::fs::remove_file(rec);
    let replayed = String::from_utf8_lossy(&back.stdout).into_owned();
    assert!(back.status.success(), "{replayed}{}", String::from_utf8_lossy(&back.stderr));
    assert_eq!(hash(&ran), hash(&replayed));
    let logs: Vec<&str> = replayed.lines().filter_map(|l| l.strip_prefix("log: ")).collect();
    assert_eq!(logs, ["true7v0", "true3v0"], "the replay gets the recorded players");
}
