//! Serialized, coalesced best-effort cache I/O away from the UI thread.
//! Up to three large source-bounded snapshots and four metadata snapshots;
//! metadata encoding is capped at 8 MiB. Count/encoded budgets aren't an RSS cap.
use crate::config::Cache;
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
};
type Task = Box<dyn FnOnce(&Cache) + Send>;
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Snapshot,
    Metadata,
    Remove,
}
#[derive(Default)]
struct Pending {
    tasks: VecDeque<(String, Task, Kind)>,
    active: Option<Kind>,
    closing: bool,
}
#[derive(Default)]
struct State {
    pending: Mutex<Pending>,
    changed: Condvar,
}
struct Handle {
    state: Arc<State>,
}
impl Drop for Handle {
    fn drop(&mut self) {
        let mut p = self.state.pending.lock().unwrap();
        p.closing = true;
        self.state.changed.notify_all();
    }
}
#[derive(Clone, Default)]
pub struct CacheWriter(Option<Arc<Handle>>);
impl CacheWriter {
    pub fn new(cache: Cache) -> Self {
        if cache.directory.is_none() {
            return Self::default();
        }
        let state = Arc::new(State::default());
        let worker = state.clone();
        if std::thread::Builder::new()
            .name("jirust-cache".into())
            .spawn(move || {
                loop {
                    let task = {
                        let mut p = worker.pending.lock().unwrap();
                        while p.tasks.is_empty() && !p.closing {
                            p = worker.changed.wait(p).unwrap();
                        }
                        let Some((_, task, kind)) = p.tasks.pop_front() else {
                            break;
                        };
                        p.active = Some(kind);
                        task
                    };
                    task(&cache);
                    let mut p = worker.pending.lock().unwrap();
                    p.active = None;
                    worker.changed.notify_all();
                }
            })
            .is_err()
        {
            return Self::default();
        }
        Self(Some(Arc::new(Handle { state })))
    }
    fn enqueue(&self, key: String, task: Task, kind: Kind) {
        let Some(handle) = &self.0 else { return };
        let mut p = handle.state.pending.lock().unwrap();
        // Move replacements to the back: never let an old write follow its removal.
        if let Some(i) = p.tasks.iter().position(|(k, _, _)| *k == key) {
            p.tasks.remove(i);
        }
        let limit = match kind {
            Kind::Snapshot => 3,
            Kind::Metadata => 4,
            Kind::Remove => crate::limits::CACHE_ENTRIES,
        };
        while p.tasks.iter().filter(|(_, _, k)| *k == kind).count()
            + usize::from(p.active == Some(kind))
            >= limit
        {
            if kind == Kind::Remove {
                return;
            } // Never evict an already queued invalidation.
            if let Some(i) = p.tasks.iter().position(|(_, _, k)| *k == kind) {
                p.tasks.remove(i);
            } else {
                return;
            }
        }
        p.tasks.push_back((key, task, kind));
        handle.state.changed.notify_all();
    }
    pub fn enabled(&self) -> bool {
        self.0.is_some()
    }
    pub fn put<T: Serialize + Send + 'static>(&self, key: String, value: T) {
        if key == crate::appearance::STATUS_COLORS_CACHE
            || key == crate::appearance::EPIC_COLORS_CACHE
        {
            self.metadata(key, value);
            return;
        }
        let name = key.clone();
        self.enqueue(
            key,
            Box::new(move |cache| {
                let _ = cache.put(&name, &value);
            }),
            Kind::Snapshot,
        );
    }
    pub fn metadata<T: Serialize + Send + 'static>(&self, key: String, value: T) {
        let name = key.clone();
        self.enqueue(
            key,
            Box::new(move |cache| {
                if crate::limits::size(&value, 8 * 1024 * 1024).is_ok() {
                    let _ = cache.put(&name, &value);
                }
            }),
            Kind::Metadata,
        );
    }
    pub fn remove(&self, key: String) {
        let name = key.clone();
        self.enqueue(
            key,
            Box::new(move |cache| cache.remove(&name)),
            Kind::Remove,
        );
    }
    /// Only call off the UI thread, or after the terminal has been restored.
    pub fn flush(&self) {
        if let Some(h) = &self.0 {
            let mut p = h.state.pending.lock().unwrap();
            while p.active.is_some() || !p.tasks.is_empty() {
                p = h.state.changed.wait(p).unwrap();
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_pressure_never_evicts_removals_or_palette_metadata_and_io_runs_on_the_worker() {
        let d = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let cache = Cache {
            directory: Some(d.path().into()),
        };
        cache.put("victim", &"old").unwrap();
        let w = CacheWriter::new(cache.clone());
        let (release, wait) = std::sync::mpsc::channel();
        let (began, ready) = std::sync::mpsc::channel();
        w.enqueue(
            "active".into(),
            Box::new(move |_| {
                assert_eq!(std::thread::current().name(), Some("jirust-cache"));
                began.send(()).unwrap();
                wait.recv().unwrap();
            }),
            Kind::Snapshot,
        );
        ready.recv().unwrap();
        w.remove("victim".into());
        w.metadata("palette".into(), vec![1, 2, 3]);
        for i in 0..10 {
            w.put(format!("snapshot-{i}"), i);
        }
        {
            let p = w.0.as_ref().unwrap().state.pending.lock().unwrap();
            assert!(
                p.tasks
                    .iter()
                    .filter(|(_, _, k)| *k == Kind::Snapshot)
                    .count()
                    <= 2
            );
        }
        release.send(()).unwrap();
        w.flush();
        assert!(!cache.exists("victim"));
        assert_eq!(cache.get::<Vec<i32>>("palette").unwrap().0, [1, 2, 3]);
    }
    #[test]
    fn latest_pending_value_wins_and_removal_cannot_resurrect_an_old_entry() {
        let d = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let c = Cache {
            directory: Some(d.path().into()),
        };
        let w = CacheWriter::new(c.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        let (started, ready) = std::sync::mpsc::channel();
        w.enqueue(
            "block".into(),
            Box::new(move |_| {
                started.send(()).unwrap();
                rx.recv().unwrap();
            }),
            Kind::Snapshot,
        );
        ready.recv().unwrap();
        w.put("ticket".into(), "old");
        w.put("ticket".into(), "new");
        tx.send(()).unwrap();
        w.flush();
        assert_eq!(c.get::<String>("ticket").unwrap().0, "new");
        w.put("ticket".into(), "stale");
        w.remove("ticket".into());
        w.flush();
        assert!(!c.exists("ticket"));
    }
}
