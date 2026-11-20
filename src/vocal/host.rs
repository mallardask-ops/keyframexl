//! Background rendering of vocal phrases.
//!
//! The UI calls [`Host::update`] with every edited song; the worker renders
//! the phrases whose notes, lyrics or settings changed (the rest come from a
//! cache keyed by content) and publishes a [`VocalMap`] the audio thread
//! reads without locking.

use super::{Bank, Phrase, VocalMap, plan::Spec, render};
use arc_swap::ArcSwap;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
};

/// Rendered audio kept for undo and repeats, in samples (≈ 200 MB).
const CACHE_SAMPLES: usize = 50_000_000;

/// What the UI shows about the renderer.
#[derive(Default)]
pub struct Status {
    /// Phrases still to render, and how many of this batch are done.
    pub total: AtomicUsize,
    pub done: AtomicUsize,
    /// Problems from the last pass: banks that failed to load, lyrics with no syllable.
    pub notes: Mutex<Vec<String>>,
}

impl Status {
    pub fn busy(&self) -> bool {
        self.done.load(Ordering::Relaxed) < self.total.load(Ordering::Relaxed)
    }
    pub fn messages(&self) -> Vec<String> {
        self.notes.lock().map(|n| n.clone()).unwrap_or_default()
    }
}

/// Banks loaded so far, by folder.
#[derive(Default)]
pub struct Banks(Mutex<HashMap<String, Result<Arc<Bank>, String>>>);

impl Banks {
    pub fn get(&self, dir: &str) -> Result<Arc<Bank>, String> {
        let mut m = self.0.lock().map_err(|_| "poisoned".to_string())?;
        if let Some(b) = m.get(dir) {
            // A failed load is retried only when the folder changes (a new Host pass).
            if b.is_ok() {
                return b.clone();
            }
        }
        let b = Bank::open(Path::new(dir));
        m.insert(dir.to_string(), b.clone());
        b
    }
    pub fn forget(&self, dir: &str) {
        if let Ok(mut m) = self.0.lock() {
            m.remove(dir);
        }
    }
}

/// Render one spec. Returns the phrase and lyrics the bank could not sing.
pub fn render_spec(banks: &Banks, spec: &Spec, rate: u32) -> Result<(Phrase, Vec<String>), String> {
    let bank = banks.get(&spec.bank)?;
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        render::render_phrase(&bank, &spec.notes, &spec.params, rate)
    }))
    .map_err(|_| "the renderer failed on this phrase".to_string())??;
    Ok((
        Phrase {
            lead: r.lead,
            data: r.data,
        },
        r.missing,
    ))
}

fn map_of(
    specs: &[Spec],
    previews: &[(u64, u8, u64, Spec)],
    cache: &HashMap<u64, Entry>,
    rate: u32,
) -> VocalMap {
    let mut m = VocalMap {
        rate,
        ..Default::default()
    };
    for (inst, pitch, lh, s) in previews {
        if let Some(e) = cache.get(&s.key)
            && !e.phrase.data.is_empty()
        {
            m.previews.insert((*inst, *pitch, *lh), e.phrase.clone());
        }
    }
    for s in specs {
        let Some(e) = cache.get(&s.key) else { continue };
        if e.phrase.data.is_empty() || s.notes.is_empty() {
            continue;
        }
        let placed = if s.song { &mut m.song } else { &mut m.pattern };
        placed
            .starts
            .insert((s.notes[0].id, s.ticks[0]), e.phrase.clone());
        placed.members.extend(
            s.notes
                .iter()
                .zip(&s.ticks)
                .skip(1)
                .map(|(n, &t)| (n.id, t)),
        );
        m.max_lead = m.max_lead.max(e.phrase.lead);
    }
    m
}

struct Entry {
    phrase: Arc<Phrase>,
    used: u64,
}

struct Msg {
    specs: Vec<Spec>,
    previews: Vec<(u64, u8, u64, Spec)>,
    rate: u32,
}

pub struct Host {
    pub map: Arc<ArcSwap<VocalMap>>,
    pub status: Arc<Status>,
    pub banks: Arc<Banks>,
    tx: Mutex<mpsc::Sender<Msg>>,
    generation: Arc<AtomicU64>,
    last: Mutex<u64>,
}

impl Host {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<Msg>();
        let map = Arc::new(ArcSwap::from_pointee(VocalMap::default()));
        let status = Arc::new(Status::default());
        let banks = Arc::new(Banks::default());
        let generation = Arc::new(AtomicU64::new(0));
        {
            let (map, status, banks, generation) = (
                map.clone(),
                status.clone(),
                banks.clone(),
                generation.clone(),
            );
            std::thread::Builder::new()
                .name("vocal-render".into())
                .spawn(move || worker(rx, map, status, banks, generation))
                .ok();
        }
        Self {
            map,
            status,
            banks,
            tx: Mutex::new(tx),
            generation,
            last: Mutex::new(0),
        }
    }

    /// Re-plan after an edit; sends work only when a phrase actually changed.
    pub fn update(&self, song: &crate::sequence::Song, rate: u32) {
        let specs = super::plan::plan(song, rate);
        let previews = super::plan::previews(song, rate);
        let mut h = 0xcbf2_9ce4_8422_2325u64 ^ specs.len() as u64;
        for (inst, _, _, s) in &previews {
            h = (h ^ s.key ^ inst).wrapping_mul(0x100_0000_01b3);
        }
        for s in &specs {
            // Position matters too: a moved phrase needs a new map entry.
            h = (h ^ s.key).wrapping_mul(0x100_0000_01b3);
            h = (h ^ s.notes[0].id).wrapping_mul(0x100_0000_01b3);
            h = (h ^ s.song as u64).wrapping_mul(0x100_0000_01b3);
            for &t in &s.ticks {
                h = (h ^ t as u64).wrapping_mul(0x100_0000_01b3);
            }
            h = (h ^ s.notes.len() as u64).wrapping_mul(0x100_0000_01b3);
            for n in &s.notes {
                h = (h ^ n.id).wrapping_mul(0x100_0000_01b3);
            }
        }
        {
            let Ok(mut last) = self.last.lock() else {
                return;
            };
            if *last == h {
                return;
            }
            *last = h;
        }
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(Msg {
                specs,
                previews,
                rate,
            });
        }
    }

    /// Forget a bank (it was reinstalled or fixed) and render again.
    pub fn reload(&self, dir: &str) {
        self.banks.forget(dir);
        if let Ok(mut l) = self.last.lock() {
            *l = 0;
        }
    }
}

impl Default for Host {
    fn default() -> Self {
        Self::new()
    }
}

fn worker(
    rx: mpsc::Receiver<Msg>,
    map: Arc<ArcSwap<VocalMap>>,
    status: Arc<Status>,
    banks: Arc<Banks>,
    generation: Arc<AtomicU64>,
) {
    let cache: Arc<Mutex<HashMap<u64, Entry>>> = Arc::new(Mutex::new(HashMap::new()));
    let tick = Arc::new(AtomicU64::new(0));
    while let Ok(mut msg) = rx.recv() {
        while let Ok(m) = rx.try_recv() {
            msg = m;
        }
        let my_gen = generation.load(Ordering::SeqCst);
        let specs = Arc::new(msg.specs);
        let previews = Arc::new(msg.previews);
        let rate = msg.rate;
        let now = tick.fetch_add(1, Ordering::SeqCst) + 1;
        // What is cached plays at once; the rest renders.
        let todo: Vec<usize> = {
            let mut c = cache.lock().unwrap();
            for s in specs.iter().chain(previews.iter().map(|p| &p.3)) {
                if let Some(e) = c.get_mut(&s.key) {
                    e.used = now;
                }
            }
            map.store(Arc::new(map_of(&specs, &previews, &c, rate)));
            let mut seen = HashSet::new();
            // The score first, then the keys you may play.
            let all: Vec<&Spec> = specs.iter().chain(previews.iter().map(|p| &p.3)).collect();
            (0..all.len())
                .filter(|i| !c.contains_key(&all[*i].key) && seen.insert(all[*i].key))
                .collect()
        };
        let all: Arc<Vec<Spec>> = Arc::new(
            specs
                .iter()
                .cloned()
                .chain(previews.iter().map(|p| p.3.clone()))
                .collect(),
        );
        status.total.store(todo.len(), Ordering::SeqCst);
        status.done.store(0, Ordering::SeqCst);
        let notes: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let next = AtomicUsize::new(0);
        let jobs = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .clamp(1, 4)
            .min(todo.len().max(1));
        std::thread::scope(|sc| {
            for _ in 0..jobs {
                sc.spawn(|| {
                    loop {
                        if generation.load(Ordering::SeqCst) != my_gen {
                            return;
                        }
                        let k = next.fetch_add(1, Ordering::SeqCst);
                        let Some(&i) = todo.get(k) else { return };
                        let spec = &all[i];
                        match render_spec(&banks, spec, rate) {
                            Ok((phrase, missing)) => {
                                let mut c = cache.lock().unwrap();
                                c.insert(
                                    spec.key,
                                    Entry {
                                        phrase: Arc::new(phrase),
                                        used: now,
                                    },
                                );
                                if generation.load(Ordering::SeqCst) == my_gen {
                                    map.store(Arc::new(map_of(&specs, &previews, &c, rate)));
                                }
                                if !missing.is_empty()
                                    && let Ok(mut n) = notes.lock()
                                {
                                    n.push(format!(
                                        "no syllable in the bank for: {}",
                                        missing.join(", ")
                                    ));
                                }
                            }
                            Err(e) => {
                                if let Ok(mut n) = notes.lock() {
                                    n.push(e);
                                }
                            }
                        }
                        status.done.fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
        });
        if generation.load(Ordering::SeqCst) == my_gen {
            if let Ok(mut n) = status.notes.lock() {
                let mut v = notes.lock().map(|v| v.clone()).unwrap_or_default();
                v.sort();
                v.dedup();
                *n = v;
            }
            status
                .done
                .store(status.total.load(Ordering::SeqCst), Ordering::SeqCst);
        }
        // Keep what undo and repeats may want, within a budget.
        let mut c = cache.lock().unwrap();
        let mut total: usize = c.values().map(|e| e.phrase.data.len()).sum();
        if total > CACHE_SAMPLES {
            let mut order: Vec<(u64, u64)> = c.iter().map(|(k, e)| (e.used, *k)).collect();
            order.sort();
            for (_, k) in order {
                if total <= CACHE_SAMPLES {
                    break;
                }
                if let Some(e) = c.remove(&k) {
                    total -= e.phrase.data.len();
                }
            }
        }
    }
}

/// Render every vocal phrase of `song` and wait (offline export, tests).
pub fn render_all(song: &crate::sequence::Song, rate: u32) -> (VocalMap, Vec<String>) {
    let specs = super::plan::plan(song, rate);
    let banks = Banks::default();
    let results: Mutex<Vec<(usize, Result<(Phrase, Vec<String>), String>)>> =
        Mutex::new(Vec::new());
    let next = AtomicUsize::new(0);
    let mut unique: Vec<usize> = Vec::new();
    let mut seen = HashSet::new();
    for (i, s) in specs.iter().enumerate() {
        if seen.insert(s.key) {
            unique.push(i);
        }
    }
    let jobs = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(1, 4)
        .min(unique.len().max(1));
    std::thread::scope(|sc| {
        for _ in 0..jobs {
            sc.spawn(|| {
                loop {
                    let k = next.fetch_add(1, Ordering::SeqCst);
                    let Some(&i) = unique.get(k) else { return };
                    let r = render_spec(&banks, &specs[i], rate);
                    results.lock().unwrap().push((i, r));
                }
            });
        }
    });
    let mut cache: HashMap<u64, Entry> = HashMap::new();
    let mut msgs = Vec::new();
    for (i, r) in results.into_inner().unwrap() {
        match r {
            Ok((p, missing)) => {
                if !missing.is_empty() {
                    msgs.push(format!(
                        "no syllable in the bank for: {}",
                        missing.join(", ")
                    ));
                }
                cache.insert(
                    specs[i].key,
                    Entry {
                        phrase: Arc::new(p),
                        used: 0,
                    },
                );
            }
            Err(e) => msgs.push(e),
        }
    }
    msgs.sort();
    msgs.dedup();
    (map_of(&specs, &[], &cache, rate), msgs)
}
