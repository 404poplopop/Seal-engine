//! Trace state is global, so everything runs in one test, in order.
use seal_trace::{counter, drain, enable, frame_mark, zone, Channel, EventKind};

#[test]
fn channels_zones_counters_and_export() {
    assert_eq!(Channel::parse("cpu, jobs").unwrap(), Channel::CPU | Channel::JOBS);
    assert_eq!(Channel::parse("all").unwrap(), Channel::ALL);
    assert!(Channel::parse("bogus").is_err());

    // Disabled: nothing recorded.
    enable(Channel::NONE);
    {
        let _z = zone!(Channel::CPU, "off");
    }
    counter(Channel::MEMORY, "bytes", 1.0);
    frame_mark();
    assert!(drain().events.is_empty());

    // Only the enabled channel records.
    enable(Channel::CPU);
    {
        let _outer = zone!(Channel::CPU, "outer");
        let _inner = zone!(Channel::CPU, "inner");
        let _io = zone!(Channel::IO, "io");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    counter(Channel::CPU, "queued", 3.0);
    std::thread::Builder::new()
        .name("worker".into())
        .spawn(|| {
            let _z = zone!(Channel::CPU, "on worker");
        })
        .unwrap()
        .join()
        .unwrap();
    frame_mark();
    let trace = drain();
    let names: Vec<&str> = trace.events.iter().map(|e| e.name).collect();
    assert!(names.contains(&"outer") && names.contains(&"inner") && names.contains(&"on worker"));
    assert!(!names.contains(&"io"));
    let outer = trace.events.iter().find(|e| e.name == "outer").unwrap();
    let inner = trace.events.iter().find(|e| e.name == "inner").unwrap();
    let (EventKind::Zone { duration_ns: od }, EventKind::Zone { duration_ns: id }) =
        (&outer.kind, &inner.kind)
    else {
        panic!()
    };
    assert!(*od >= 2_000_000 && od >= id && outer.start_ns <= inner.start_ns, "nested zones");
    assert!(trace.threads.iter().any(|(_, n)| n == "worker"));
    assert_eq!(trace.summary()[0].0, "outer");

    let json: serde_json::Value = serde_json::from_str(&trace.to_chrome_json()).unwrap();
    let events = json["traceEvents"].as_array().unwrap();
    assert!(events.iter().any(|e| e["ph"] == "X" && e["name"] == "outer"));
    assert!(events.iter().any(|e| e["ph"] == "C" && e["args"]["value"] == 3.0));
    assert!(events.iter().any(|e| e["ph"] == "M" && e["args"]["name"] == "worker"));
    assert!(drain().events.is_empty(), "drain empties the buffers");

    // A full buffer drops the oldest events and counts them.
    seal_trace::set_capacity_per_thread(4);
    for _ in 0..10 {
        let _z = zone!(Channel::CPU, "spam");
    }
    let trace = drain();
    assert_eq!(trace.events.len(), 4);
    assert_eq!(trace.dropped, 6);
    assert_eq!(seal_trace::intern("sys"), seal_trace::intern(&String::from("sys")));
    enable(Channel::NONE);
}
