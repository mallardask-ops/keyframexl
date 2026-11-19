//! Bridge tests with real child processes: the test binary re-runs itself
//! with only `child_entry` selected and `KFXL_BRIDGE_CHILD` set, and that
//! "test" becomes the plugin-host child.

use super::*;
use std::time::{Duration, Instant};

const RATE: u32 = 48_000;

/// `plugin::OFFLINE` is process-wide: tests that process blocks take turns.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}
const FRAMES: usize = 256;

#[test]
fn child_entry() {
    if std::env::var_os("KFXL_BRIDGE_CHILD").is_none() {
        return;
    }
    let _ = child_main();
    std::process::exit(0);
}

fn child_command() -> Command {
    let mut c = Command::new(std::env::current_exe().unwrap());
    c.args([
        "bridge::tests::child_entry",
        "--exact",
        "--nocapture",
        "--test-threads=1",
        "-q",
    ]);
    c.env("KFXL_BRIDGE_CHILD", "1");
    c.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    c
}

fn transport() -> Transport {
    Transport {
        rate: RATE,
        tempo: 120.0,
        playing: true,
        frame: 0,
    }
}

/// One block, retried until the child had time to answer the previous one
/// (tests run without a realtime clock, so wait a little instead of counting xruns).
fn run(p: &mut BridgeProc, events: &[PluginEvent], io: &mut [f32]) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while p.sup.shm.header().out_seq.load(Ordering::Acquire) != p.published
        && Instant::now() < deadline
        && !p.crashed()
    {
        std::thread::sleep(Duration::from_micros(200));
    }
    p.process(events, io, &transport());
}

fn ramp(n: usize, k: f32) -> Vec<f32> {
    (0..n * 2).map(|i| (i as f32 + k) / 1000.0).collect()
}

#[test]
fn gain_comes_back_one_block_later() {
    let _turn = serial();
    let mut p = BridgeProc::start(child_command(), RATE, FRAMES as u32, "gain:0.5").unwrap();
    let a = ramp(FRAMES, 0.0);
    let b = ramp(FRAMES, 1.0);
    let mut io = a.clone();
    run(&mut p, &[], &mut io);
    assert!(
        io.iter().all(|s| *s == 0.0),
        "the first block is the bridge's latency"
    );
    let mut io = b.clone();
    run(&mut p, &[], &mut io);
    let want: Vec<f32> = a.iter().map(|s| s * 0.5).collect();
    assert_eq!(io, want, "block N plays block N-1, processed");
    assert!(!p.crashed());
}

#[test]
fn a_note_starts_at_its_frame() {
    let _turn = serial();
    let mut p = BridgeProc::start(child_command(), RATE, FRAMES as u32, "synth").unwrap();
    let mut io = vec![0.0; FRAMES * 2];
    run(
        &mut p,
        &[PluginEvent::NoteOn {
            at: 100,
            pitch: 69,
            velocity: 127,
        }],
        &mut io,
    );
    let mut io = vec![0.0; FRAMES * 2];
    run(&mut p, &[], &mut io);
    let peak = io.iter().fold(0f32, |m, s| m.max(s.abs()));
    let first = io
        .chunks(2)
        .position(|f| f[0].abs() > 1e-6)
        .unwrap_or_else(|| {
            panic!(
                "no sound, peak {peak}, n_events {}",
                p.sup.shm.header().n_events.load(Ordering::Relaxed)
            )
        });
    // The sine starts at phase 0, so frame 100 is exactly 0 and 101 is the first sound.
    assert!(
        (100..=101).contains(&first),
        "sound starts at the note's frame, got {first}"
    );
}

#[test]
fn a_crash_mid_block_goes_silent_and_is_reported() {
    let _turn = serial();
    let mut p = BridgeProc::start(child_command(), RATE, FRAMES as u32, "crash-after:2").unwrap();
    for _ in 0..3 {
        let mut io = ramp(FRAMES, 0.0);
        run(&mut p, &[], &mut io);
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while !p.crashed() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(p.crashed(), "the watcher saw the child die");
    assert!(
        matches!(p.fault(), Some(Fault::Exited(_))),
        "fault {:?}",
        p.fault()
    );
    let mut io = ramp(FRAMES, 0.0);
    p.process(&[], &mut io, &transport());
    assert!(io.iter().all(|s| *s == 0.0), "a crashed instance is silent");
}

#[test]
fn a_hang_is_silent_then_killed() {
    let _turn = serial();
    let mut p = BridgeProc::start(child_command(), RATE, FRAMES as u32, "hang").unwrap();
    let mut io = ramp(FRAMES, 0.0);
    p.process(&[], &mut io, &transport());
    // The child never answers: the next blocks are silent and count as xruns.
    let mut io = ramp(FRAMES, 0.0);
    p.process(&[], &mut io, &transport());
    assert!(io.iter().all(|s| *s == 0.0));
    assert!(p.xruns.load(Ordering::Relaxed) >= 1);
    let deadline = Instant::now() + supervisor::HANG_AFTER + Duration::from_secs(2);
    while !p.crashed() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(p.fault(), Some(Fault::Hung));
}

#[test]
fn bad_plugin_and_long_blocks() {
    let _turn = serial();
    assert!(
        BridgeProc::start(child_command(), RATE, FRAMES as u32, "no-such-plugin").is_err(),
        "a load error comes back as an error"
    );
    // A block longer than the shared one is split, latency stays one piece.
    let mut p = BridgeProc::start(child_command(), RATE, FRAMES as u32, "gain:1").unwrap();
    let mut io = ramp(FRAMES * 3, 0.0);
    let orig = io.clone();
    // Each piece waits for the previous one inside process() only via the test's retry, so feed pieces one by one.
    for k in 0..3 {
        run(&mut p, &[], &mut io[k * FRAMES * 2..(k + 1) * FRAMES * 2]);
    }
    assert!(io[..FRAMES * 2].iter().all(|s| *s == 0.0));
    assert_eq!(&io[FRAMES * 2..], &orig[..FRAMES * 4]);
}

#[test]
fn offline_blocks_come_back_at_once() {
    let _turn = serial();
    let mut p = BridgeProc::start(child_command(), RATE, FRAMES as u32, "gain:2").unwrap();
    keyframexl::plugin::OFFLINE.store(true, Ordering::Release);
    let a = ramp(FRAMES, 0.0);
    let mut io = a.clone();
    p.process(&[], &mut io, &transport());
    keyframexl::plugin::OFFLINE.store(false, Ordering::Release);
    let want: Vec<f32> = a.iter().map(|s| s * 2.0).collect();
    assert_eq!(
        io, want,
        "an export gets the block itself, no latency, no waiting in the test"
    );
    // Back live: the pipeline restarts cleanly (one block of silence, then audio).
    let mut io = a.clone();
    run(&mut p, &[], &mut io);
    assert!(io.iter().all(|s| *s == 0.0));
    let mut io = a.clone();
    run(&mut p, &[], &mut io);
    assert_eq!(io, want);
}

#[test]
fn a_multi_out_plugins_extra_pairs_come_back_with_the_main_one() {
    let _turn = serial();
    let mut p = BridgeProc::start(child_command(), RATE, FRAMES as u32, "multi:2").unwrap();
    let stride = FRAMES * 2;
    let mut extra = vec![0.0; stride * 3];
    let a = ramp(FRAMES, 0.0);
    let mut io = a.clone();
    let deadline = Instant::now() + Duration::from_secs(10);
    let wait = |p: &BridgeProc| {
        while p.sup.shm.header().out_seq.load(Ordering::Acquire) != p.published
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_micros(200));
        }
    };
    wait(&p);
    p.process_multi(&[], &mut io, &mut extra, stride, &transport());
    assert!(io.iter().all(|s| *s == 0.0), "latency block");
    wait(&p);
    let mut io = ramp(FRAMES, 1.0);
    let wrote = p.process_multi(&[], &mut io, &mut extra, stride, &transport());
    assert_eq!(wrote, 2, "the child reported two extra pairs");
    assert_eq!(p.extra_pairs(), 2);
    assert_eq!(p.latency(FRAMES), FRAMES, "live: one block behind");
    assert_eq!(io, a, "main pair is block N-1");
    assert!(extra[..stride].iter().all(|s| *s == 0.25));
    assert!(extra[stride..2 * stride].iter().all(|s| *s == 0.5));
    assert!(
        extra[2 * stride..].iter().all(|s| *s == 0.0),
        "no third pair"
    );
    // Offline (export): no latency, the pairs come with their own block.
    keyframexl::plugin::OFFLINE.store(true, Ordering::Release);
    extra.fill(9.0);
    let mut io = ramp(FRAMES, 2.0);
    let wrote = p.process_multi(&[], &mut io, &mut extra, stride, &transport());
    assert_eq!(p.latency(FRAMES), 0, "offline: no pipeline delay");
    keyframexl::plugin::OFFLINE.store(false, Ordering::Release);
    assert_eq!(wrote, 2);
    assert_eq!(io, ramp(FRAMES, 2.0));
    assert!(extra[stride..2 * stride].iter().all(|s| *s == 0.5));
}

#[test]
fn a_sidechain_key_reaches_the_child_beside_the_signal() {
    let _turn = serial();
    let mut p = BridgeProc::start(child_command(), RATE, FRAMES as u32, "key").unwrap();
    keyframexl::plugin::OFFLINE.store(true, Ordering::Release);
    let key = ramp(FRAMES, 7.0);
    let mut io = ramp(FRAMES, 0.0);
    p.process_keyed(&[], &mut io, &key, &transport());
    let keyed = io.clone();
    let mut io = ramp(FRAMES, 0.0);
    p.process(&[], &mut io, &transport());
    // Longer than the shared block: the key is cut into the same pieces.
    let long_key = ramp(FRAMES * 3, 1.0);
    let mut long = vec![0.5; FRAMES * 3 * 2];
    p.process_keyed(&[], &mut long, &long_key, &transport());
    keyframexl::plugin::OFFLINE.store(false, Ordering::Release);
    assert_eq!(keyed, key, "the child heard the key");
    assert!(io.iter().all(|s| *s == 0.0), "no key, nothing to hear");
    assert_eq!(long, long_key);
}
