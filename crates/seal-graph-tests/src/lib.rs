//! Graphs from `graphs/`, compiled to Rust by `build.rs`.

include!(concat!(env!("OUT_DIR"), "/graphs.rs"));

/// The source text of each graph, for running the same graph in the interpreter.
pub const SOURCES: &[(&str, &str)] = &[
    ("census", include_str!("../graphs/census.graph")),
    ("clock", include_str!("../graphs/clock.graph")),
    ("counter", include_str!("../graphs/counter.graph")),
    ("dice", include_str!("../graphs/dice.graph")),
    ("early", include_str!("../graphs/early.graph")),
    ("fizz", include_str!("../graphs/fizz.graph")),
    ("fnbody", include_str!("../graphs/fnbody.graph")),
    ("funcs", include_str!("../graphs/funcs.graph")),
    ("kinds", include_str!("../graphs/kinds.graph")),
    ("lists", include_str!("../graphs/lists.graph")),
    ("mathops", include_str!("../graphs/mathops.graph")),
    ("mover", include_str!("../graphs/mover.graph")),
    ("numparse", include_str!("../graphs/numparse.graph")),
    ("numtext", include_str!("../graphs/numtext.graph")),
    ("players", include_str!("../graphs/players.graph")),
    ("pulse", include_str!("../graphs/pulse.graph")),
    ("queries", include_str!("../graphs/queries.graph")),
    ("rules", include_str!("../graphs/rules.graph")),
    ("space", include_str!("../graphs/space.graph")),
    ("spawner", include_str!("../graphs/spawner.graph")),
    ("states", include_str!("../graphs/states.graph")),
    ("strops", include_str!("../graphs/strops.graph")),
    ("strsearch", include_str!("../graphs/strsearch.graph")),
    ("swarm", include_str!("../graphs/swarm.graph")),
    ("timers", include_str!("../graphs/timers.graph")),
    ("transc", include_str!("../graphs/transc.graph")),
    ("vectext", include_str!("../graphs/vectext.graph")),
];
