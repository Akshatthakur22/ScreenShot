use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use notify::event::{AccessKind, AccessMode, ModifyKind, RenameMode};
use notify::{Event, EventKind, RecursiveMode, Watcher};

use akshat_core::Config;

use crate::indexer::{self, Indexer, Outcome};

// How long a file has to sit untouched before it gets read. Some tools write a
// screenshot in several passes, and reading a half written png just fails.
const SETTLE: Duration = Duration::from_millis(400);

struct Watch {
    indexer: Indexer,
    /// Files that changed recently, read once they've been quiet for SETTLE.
    pending: HashMap<PathBuf, Instant>,
    /// Files that were renamed away. inotify reports the old name first and
    /// only pairs it with the new one in a later event, so forgetting right
    /// away would throw out the text of a file that only moved.
    leaving: HashMap<PathBuf, Instant>,
}

/// Folders given on the command line are fixed. Without them the watcher
/// follows the config file, so adding or removing a folder in the app takes
/// effect here straight away, no restart.
pub fn run(fixed: Option<Vec<PathBuf>>, threads: Option<usize>) -> Result<()> {
    indexer::become_idle();
    let config_path = Config::path()?;
    let config = Config::load_or_default();
    let mut threads_now = threads.unwrap_or(config.threads);
    let mut folders = fixed.clone().unwrap_or(config.folders);

    let mut w = Watch {
        indexer: Indexer::new(threads_now)?,
        pending: HashMap::new(),
        leaving: HashMap::new(),
    };

    for path in w.indexer.index.paths()? {
        if !path.exists() {
            w.indexer.forget(&path)?;
        }
    }

    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx)?;
    for dir in &folders {
        watch_folder(&mut watcher, dir);
    }
    let follow_config = fixed.is_none();
    if follow_config && let Some(dir) = config_path.parent() {
        std::fs::create_dir_all(dir)?;
        watcher.watch(dir, RecursiveMode::NonRecursive)?;
    }

    let mut backlog: VecDeque<PathBuf> = indexer::scan(&folders).into();
    let mut caught_up = backlog.is_empty();
    let mut power = Power::default();

    loop {
        let mut config_changed = false;
        let mut take = |event: notify::Result<Event>, w: &mut Watch| -> Result<()> {
            let event = event?;
            config_changed |= follow_config && event.paths.iter().any(|p| p == &config_path);
            w.on_event(event);
            Ok(())
        };
        // Block for as long as there is nothing else to do.
        let wait = if !w.pending.is_empty() || !w.leaving.is_empty() {
            SETTLE
        } else if !backlog.is_empty() {
            // Working through an old library can wait, a laptop's battery
            // matters more. Anything new still wakes this straight away.
            if power.on_battery() {
                BATTERY_PACE
            } else {
                Duration::ZERO
            }
        } else {
            Duration::from_secs(3600)
        };
        match rx.recv_timeout(wait) {
            Ok(event) => take(event, &mut w)?,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => bail!("file watcher stopped"),
        }
        while let Ok(event) = rx.try_recv() {
            take(event, &mut w)?;
        }

        // A config that doesn't parse (someone mid edit) is ignored until it does.
        if config_changed && let Ok(Some(config)) = Config::load_from(&config_path) {
            let added: Vec<PathBuf> = config
                .folders
                .iter()
                .filter(|f| !folders.contains(f))
                .cloned()
                .collect();
            for gone in folders.iter().filter(|f| !config.folders.contains(f)) {
                let _ = watcher.unwatch(gone);
                backlog.retain(|p| {
                    !p.starts_with(gone) || config.folders.iter().any(|f| p.starts_with(f))
                });
                let dropped = w.forget_under(gone, &config.folders);
                eprintln!(
                    "stopped watching {} ({dropped} screenshots forgotten)",
                    gone.display()
                );
            }
            for dir in &added {
                watch_folder(&mut watcher, dir);
                // Newly added folders go to the front, it's what was just asked for.
                for path in indexer::scan(std::slice::from_ref(dir)).into_iter().rev() {
                    backlog.push_front(path);
                }
                caught_up = false;
            }
            if threads.is_none() && config.threads != threads_now {
                threads_now = config.threads;
                w.indexer.set_threads(threads_now)?;
                eprintln!("reading with {threads_now} threads now");
            }
            folders = config.folders;
        }

        for path in settled(&mut w.leaving) {
            if !path.exists() {
                forget(&mut w.indexer, &path);
            }
        }

        // Fresh screenshots jump the queue: the one you took just now is the
        // one you're about to look for, even while an old library backfills.
        let fresh = settled(&mut w.pending);
        if !fresh.is_empty() {
            for path in fresh.iter().filter(|p| p.is_file()) {
                index(&mut w.indexer, path);
            }
            continue;
        }

        if let Some(path) = backlog.pop_front() {
            index(&mut w.indexer, &path);
        } else if !caught_up {
            caught_up = true;
            eprintln!(
                "caught up, {} screenshots searchable",
                w.indexer.index.visible_len()?
            );
        }
    }
}

// On battery, one old screenshot every few seconds instead of flat out.
const BATTERY_PACE: Duration = Duration::from_secs(3);

/// Whether the machine is running on battery, looked up at most every 30
/// seconds. Any laptop's kernel lists its batteries under
/// /sys/class/power_supply, and one that says Discharging means unplugged.
/// Desktops have none, so they're never throttled.
#[derive(Default)]
struct Power {
    checked: Option<Instant>,
    on_battery: bool,
}

impl Power {
    fn on_battery(&mut self) -> bool {
        if self
            .checked
            .is_none_or(|t| t.elapsed() > Duration::from_secs(30))
        {
            self.checked = Some(Instant::now());
            self.on_battery = std::fs::read_dir("/sys/class/power_supply")
                .into_iter()
                .flatten()
                .flatten()
                .any(|supply| {
                    let read = |f: &str| {
                        std::fs::read_to_string(supply.path().join(f)).unwrap_or_default()
                    };
                    read("type").trim() == "Battery" && read("status").trim() == "Discharging"
                });
        }
        self.on_battery
    }
}

/// A folder that isn't there (an unplugged drive, a typo in the config) is
/// reported and skipped rather than taking the whole watcher down.
fn watch_folder(watcher: &mut impl Watcher, dir: &Path) {
    match watcher.watch(dir, RecursiveMode::Recursive) {
        Ok(()) => eprintln!("watching {}", dir.display()),
        Err(e) => eprintln!("can't watch {}: {e}", dir.display()),
    }
}

/// Takes out every entry that's been quiet for at least SETTLE.
fn settled(map: &mut HashMap<PathBuf, Instant>) -> Vec<PathBuf> {
    let now = Instant::now();
    let ready: Vec<PathBuf> = map
        .iter()
        .filter(|(_, t)| now.duration_since(**t) >= SETTLE)
        .map(|(p, _)| p.clone())
        .collect();
    for p in &ready {
        map.remove(p);
    }
    ready
}

impl Watch {
    /// Forgets every shot under `dir` that isn't also under one of `keep`
    /// (folders can nest). Returns how many.
    fn forget_under(&mut self, dir: &Path, keep: &[PathBuf]) -> usize {
        let paths = self.indexer.index.paths().unwrap_or_default();
        let gone: Vec<PathBuf> = paths
            .into_iter()
            .filter(|p| p.starts_with(dir) && !keep.iter().any(|k| p.starts_with(k)))
            .collect();
        for p in &gone {
            self.pending.remove(p);
            let _ = self.indexer.forget(p);
        }
        gone.len()
    }

    fn on_event(&mut self, event: Event) {
        let now = Instant::now();
        let images = event.paths.iter().filter(|p| indexer::is_image(p));

        match event.kind {
            EventKind::Remove(_) => {
                for path in images {
                    self.pending.remove(path);
                    forget(&mut self.indexer, path);
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                for path in images {
                    self.pending.remove(path);
                    self.leaving.insert(path.clone(), now);
                }
            }
            // A rename inside the watched folders, finally with both names.
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() == 2 => {
                let (from, to) = (&event.paths[0], &event.paths[1]);
                self.leaving.remove(from);
                if indexer::is_image(from) && indexer::is_image(to) {
                    match self.indexer.rename(from, to) {
                        Ok(true) => eprintln!("moved {} -> {}", from.display(), to.display()),
                        Ok(false) => {
                            self.pending.insert(to.clone(), now);
                        }
                        Err(e) => eprintln!("failed to move {}: {e:#}", from.display()),
                    }
                } else if indexer::is_image(from) {
                    forget(&mut self.indexer, from);
                } else if indexer::is_image(to) {
                    self.pending.insert(to.clone(), now);
                }
            }
            // The writer closed the file, so it's complete and there's nothing
            // to wait for. Marking it as already settled gets a fresh
            // screenshot read straight away instead of 400 ms later.
            EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
                let done = now.checked_sub(SETTLE).unwrap_or(now);
                for path in images {
                    self.pending.insert(path.clone(), done);
                }
            }
            EventKind::Create(_) | EventKind::Modify(_) => {
                for path in images {
                    self.pending.insert(path.clone(), now);
                }
            }
            _ => {}
        }
    }
}

fn index(indexer: &mut Indexer, path: &Path) {
    match indexer.index_file(path) {
        Ok(Outcome::Indexed { lines, took }) => {
            eprintln!("indexed {} ({lines} lines, {took:.1?})", path.display());
        }
        Ok(Outcome::Hidden(why)) => eprintln!("skipped {}: {why}", path.display()),
        Ok(Outcome::Unchanged | Outcome::Thumbnail) => {}
        // One bad file shouldn't take the watcher down with it.
        Err(e) => eprintln!("failed {}: {e:#}", path.display()),
    }
}

fn forget(indexer: &mut Indexer, path: &Path) {
    match indexer.forget(path) {
        Ok(true) => eprintln!("forgot {}", path.display()),
        Ok(false) => {}
        Err(e) => eprintln!("failed to forget {}: {e:#}", path.display()),
    }
}
