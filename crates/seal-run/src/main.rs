//! `seal-run`: the headless Seal runtime.
//!
//! `seal-run scale [--entities N] [--frames F] [--chunk C] [--max-threads T]`
//! runs the scaling suite: a particle world stepped with 1, 2, 4 … T worker
//! threads. It prints the time per frame, the speed-up, and a checksum of
//! the final state. The checksum must be identical for every thread count;
//! the run fails (exit code 1) if it is not. All simulation maths goes
//! through `seal-math`, so the checksum also matches across machines and
//! operating systems.
//!
//! `--trace FILE` records every frame, phase and job and writes a trace that
//! Perfetto or chrome://tracing opens; `--trace-channels cpu,jobs,memory`
//! picks channels (default all). `--tracy` streams to a Tracy viewer when
//! built with the `tracy` feature.

use std::process::ExitCode;
use std::time::Instant;

use seal_ecs::World;
use seal_jobs::JobPool;
use seal_mem::{tagged, MemTag, TrackingAllocator};

#[global_allocator]
static ALLOC: TrackingAllocator<std::alloc::System> = TrackingAllocator::new(std::alloc::System);

#[derive(Clone, Copy, PartialEq)]
struct Position([f32; 3]);
#[derive(Clone, Copy, PartialEq)]
struct Velocity([f32; 3]);
#[derive(Clone, Copy, PartialEq)]
struct Bounce(f32);
/// A second archetype: half the particles also carry a spin.
#[derive(Clone, Copy, PartialEq)]
struct Spin(f32);

struct Options {
    entities: usize,
    frames: usize,
    chunk: usize,
    max_threads: usize,
    trace: Option<String>,
    channels: seal_trace::Channel,
    tracy: bool,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        entities: 1_000_000,
        frames: 60,
        chunk: 4096,
        max_threads: std::thread::available_parallelism().map_or(1, |n| n.get()),
        trace: None,
        channels: seal_trace::Channel::ALL,
        tracy: false,
    };
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let mut value = |name: &str| -> Result<usize, String> {
            iter.next()
                .and_then(|v| v.replace('_', "").parse().ok())
                .ok_or_else(|| format!("{name} needs a number"))
        };
        match flag.as_str() {
            "--trace" => {
                options.trace = Some(iter.next().ok_or("--trace needs a file")?.clone());
                continue;
            }
            "--trace-channels" => {
                options.channels = seal_trace::Channel::parse(
                    iter.next().ok_or("--trace-channels needs a list")?,
                )?;
                continue;
            }
            "--tracy" => {
                if !seal_trace::start_tracy() {
                    return Err("--tracy needs seal-run built with --features tracy".into());
                }
                options.tracy = true;
                continue;
            }
            _ => {}
        }
        match flag.as_str() {
            "--entities" => options.entities = value("--entities")?,
            "--frames" => options.frames = value("--frames")?,
            "--chunk" => options.chunk = value("--chunk")?,
            "--max-threads" => options.max_threads = value("--max-threads")?.max(1),
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(options)
}

fn build_world(entities: usize) -> World {
    tagged(MemTag::World, || {
        let mut world = World::new();
        let mut seed = 0x9E37_79B9u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        let half = entities / 2;
        world.spawn_batch((0..half).map(|_| {
            (
                Position([next() * 50.0, next() * 50.0 + 60.0, next() * 50.0]),
                Velocity([next(), next(), next()]),
                Bounce(0.8),
            )
        }));
        world.spawn_batch((half..entities).map(|_| {
            (
                Position([next() * 50.0, next() * 50.0 + 60.0, next() * 50.0]),
                Velocity([next(), next(), next()]),
                Bounce(0.6),
                Spin(next()),
            )
        }));
        world
    })
}

const DT: f32 = 1.0 / 60.0;

fn step(world: &mut World, pool: &JobPool, chunk: usize) {
    let _frame = seal_trace::zone!(seal_trace::Channel::CPU, "frame");
    world.advance_tick();
    let gravity = seal_trace::zone!(seal_trace::Channel::CPU, "gravity and bounce");
    world.query::<(&mut Position, &mut Velocity, &Bounce)>().par_for_each(
        pool,
        chunk,
        |(mut p, mut v, b)| {
            v.0[1] -= 9.81 * DT;
            for axis in 0..3 {
                p.0[axis] += v.0[axis] * DT;
            }
            if p.0[1] < 0.0 {
                p.0[1] = -p.0[1];
                v.0[1] = -v.0[1] * b.0;
            }
        },
    );
    drop(gravity);
    let _spin = seal_trace::zone!(seal_trace::Channel::CPU, "spin");
    world.query::<(&mut Velocity, &Spin)>().par_for_each(pool, chunk, |(mut v, s)| {
        let (sin, cos) = seal_math::sin_cos(s.0 * DT);
        let [x, y, z] = v.0;
        v.0 = [x * cos - z * sin, y, x * sin + z * cos];
    });
}

/// Order-independent of thread count: chunk sums are folded in row order.
fn checksum(world: &mut World, pool: &JobPool, chunk: usize) -> u64 {
    let parts = world.query::<(&Position, &Velocity)>().par_map_chunks(pool, chunk, |rows| {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for (p, v) in rows {
            for value in p.0.iter().chain(v.0.iter()) {
                hash = (hash ^ u64::from(value.to_bits())).wrapping_mul(0x0100_0000_01b3);
            }
        }
        hash
    });
    parts.into_iter().fold(0u64, |acc, h| acc.rotate_left(5) ^ h)
}

fn scale(options: Options) -> ExitCode {
    let mut counts = vec![1usize];
    while counts.last().unwrap() * 2 <= options.max_threads {
        counts.push(counts.last().unwrap() * 2);
    }
    if *counts.last().unwrap() != options.max_threads {
        counts.push(options.max_threads);
    }
    println!(
        "seal-run scale: {} entities in 2 archetypes, {} frames, {} rows per job, {} hardware threads",
        options.entities,
        options.frames,
        options.chunk,
        std::thread::available_parallelism().map_or(1, |n| n.get())
    );
    println!(
        "{:>8} {:>12} {:>9} {:>11}  checksum",
        "threads", "ms/frame", "speed-up", "efficiency"
    );
    let mut baseline = None;
    let mut reference = None;
    let mut deterministic = true;
    for &threads in &counts {
        let mut world = build_world(options.entities);
        // The calling thread helps too, so a pool of n-1 workers runs n threads.
        let pool = JobPool::new(threads - 1);
        step(&mut world, &pool, options.chunk); // warm-up
        if options.trace.is_some() || options.tracy {
            seal_trace::enable(options.channels);
        }
        let start = Instant::now();
        for _ in 0..options.frames {
            step(&mut world, &pool, options.chunk);
            seal_trace::counter(
                seal_trace::Channel::MEMORY,
                "heap bytes",
                seal_mem::snapshot().iter().map(|u| u.live_bytes).sum::<u64>() as f64,
            );
            seal_trace::frame_mark();
        }
        seal_trace::enable(seal_trace::Channel::NONE);
        let ms = start.elapsed().as_secs_f64() * 1000.0 / options.frames as f64;
        let sum = checksum(&mut world, &pool, options.chunk);
        let base = *baseline.get_or_insert(ms);
        let speedup = base / ms;
        let same = *reference.get_or_insert(sum) == sum;
        deterministic &= same;
        println!(
            "{threads:>8} {ms:>12.3} {speedup:>8.2}x {:>10.0}%  {sum:016x}{}",
            speedup / threads as f64 * 100.0,
            if same { "" } else { "  MISMATCH" }
        );
    }
    if let Some(path) = &options.trace {
        let trace = seal_trace::drain();
        if let Err(e) = std::fs::write(path, trace.to_chrome_json()) {
            eprintln!("seal-run: {path}: {e}");
            return ExitCode::from(2);
        }
        let jobs = trace.summary().into_iter().find(|z| z.0 == "job").map_or(0, |z| z.2);
        println!(
            "trace: {} events, {jobs} jobs, {} dropped, written to {path}",
            trace.events.len(),
            trace.dropped
        );
        for (name, ns, count) in trace.summary().into_iter().filter(|z| z.0 != "job").take(4) {
            println!("  {name:<20} {:>10.3} ms total over {count}", ns as f64 / 1e6);
        }
    }
    let world_mem = seal_mem::snapshot()
        .into_iter()
        .find(|u| u.tag == MemTag::World)
        .map_or(0, |u| u.peak_bytes);
    println!("peak world memory: {:.1} MiB", world_mem as f64 / (1024.0 * 1024.0));
    if deterministic {
        println!("deterministic: yes (identical state for every thread count)");
        ExitCode::SUCCESS
    } else {
        println!("deterministic: NO");
        ExitCode::FAILURE
    }
}

/// Git merge driver for scene files:
/// `git config merge.seal-scene.driver "seal-run merge-scene %O %A %B"` and
/// `*.sealscene merge=seal-scene` in .gitattributes. Writes the result over
/// `ours`; exits 1 when there are conflicts so git marks the file conflicted.
fn merge_scene(paths: &[String]) -> ExitCode {
    let [base, ours, theirs] = paths else {
        eprintln!("usage: seal-run merge-scene BASE OURS THEIRS");
        return ExitCode::from(2);
    };
    let read = |p: &String| std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"));
    let result = (|| {
        let outcome = seal_ecs::merge_scenes(&read(base)?, &read(ours)?, &read(theirs)?)?;
        std::fs::write(ours, &outcome.text).map_err(|e| format!("{ours}: {e}"))?;
        Ok::<_, String>(outcome.conflicts)
    })();
    match result {
        Ok(conflicts) if conflicts.is_empty() => ExitCode::SUCCESS,
        Ok(conflicts) => {
            for line in &conflicts {
                eprintln!("seal-run: conflict: {line} (kept ours)");
            }
            ExitCode::from(1)
        }
        Err(message) => {
            eprintln!("seal-run: {message}");
            ExitCode::from(2)
        }
    }
}

/// Load a scene file and print what it holds, or why it does not load.
fn check_scene(paths: &[String]) -> ExitCode {
    let [path] = paths else {
        eprintln!("usage: seal-run check-scene FILE");
        return ExitCode::from(2);
    };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("seal-run: {path}: {e}");
            return ExitCode::from(2);
        }
    };
    let mut world = seal_scene::scene_world();
    match world.load_scene(&text) {
        Ok(entities) => {
            let roots = entities.iter().filter(|e| world.parent(**e).is_none()).count();
            let settings =
                world.resource::<seal_ecs::core::SceneSettings>().cloned().unwrap_or_default();
            println!("{path}: {} entities ({roots} at the top level)", entities.len());
            println!("gravity {} time scale {}", settings.gravity, settings.time_scale);
            if !settings.music.is_empty() {
                println!("music {}", settings.music);
            }
            let mut counts = std::collections::BTreeMap::new();
            for name in world.text_component_names() {
                let n =
                    entities.iter().filter(|e| world.component_value(**e, name).is_some()).count();
                if n > 0 {
                    counts.insert(name, n);
                }
            }
            for (name, n) in counts {
                println!("  {name}: {n}");
            }
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("seal-run: {path}: {message}");
            ExitCode::FAILURE
        }
    }
}

mod graph;
mod package;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("scale") => match parse(&args[1..]) {
            Ok(options) => scale(options),
            Err(message) => {
                eprintln!("seal-run: {message}");
                ExitCode::from(2)
            }
        },
        Some("merge-scene") => merge_scene(&args[1..]),
        Some("check-scene") => check_scene(&args[1..]),
        Some("graph") => graph::main(&args[1..]),
        Some("package") => match package::parse(&args[1..]).and_then(|o| package::package(&o)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("seal-run package: {message}");
                ExitCode::FAILURE
            }
        },
        Some("verify-package") => {
            match args.get(1).map(|d| package::verify(std::path::Path::new(d))) {
                Some(Ok(problems)) if problems.is_empty() => {
                    println!("package intact");
                    ExitCode::SUCCESS
                }
                Some(Ok(problems)) => {
                    for p in problems {
                        eprintln!("{p}");
                    }
                    ExitCode::FAILURE
                }
                Some(Err(message)) => {
                    eprintln!("seal-run verify-package: {message}");
                    ExitCode::FAILURE
                }
                None => {
                    eprintln!("usage: seal-run verify-package DIR");
                    ExitCode::from(2)
                }
            }
        }
        _ => {
            eprintln!(
                "usage: seal-run scale [--entities N] [--frames F] [--chunk C] [--max-threads T]"
            );
            eprintln!("                      [--trace FILE] [--trace-channels LIST] [--tracy]");
            eprintln!("       seal-run merge-scene BASE OURS THEIRS");
            eprintln!("       seal-run check-scene FILE");
            eprintln!("       seal-run graph check|compile|run ...");
            eprintln!("       seal-run package --bin NAME [-p PACKAGE] [--data DIR] [--out DIR] [--name NAME]");
            eprintln!("                        [--toolchain T] [--target TRIPLE] [--target-dir DIR] [--no-build]");
            eprintln!("       seal-run verify-package DIR");
            ExitCode::from(2)
        }
    }
}
