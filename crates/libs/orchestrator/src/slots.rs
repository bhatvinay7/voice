//! Cell-local admission authority. Scheduler capacity is only a hint.
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sled::transaction::Transactional;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::watch;

#[derive(Clone, Serialize, Deserialize)]
struct Route {
    call_id: String,
    registered_at: i64,
    last_heartbeat: i64,
    expiring: bool,
}
struct Inner {
    free: sled::Tree,
    live: sled::Tree,
}
#[derive(Clone)]
pub struct Slots {
    inner: Arc<Mutex<Inner>>,
    pub capacity: u32,
    draining: Arc<AtomicBool>,
    changes: watch::Sender<u64>,
}
impl Slots {
    pub fn new(db: &sled::Db, capacity: u32) -> sled::Result<Self> {
        let free = db.open_tree("cell_free_slots")?;
        let live = db.open_tree("cell_live_routes")?;
        // Actors and provider sockets cannot survive a process restart. Never resurrect
        // their old routes from local disk; admission starts a new process incarnation.
        free.clear()?;
        live.clear()?;
        for slot in 0..capacity {
            free.insert(slot.to_be_bytes(), &[])?;
        }
        let (changes, _) = watch::channel(0);
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner { free, live })),
            capacity,
            draining: Arc::new(AtomicBool::new(false)),
            changes,
        })
    }
    fn changed(&self) {
        self.changes.send_modify(|v| *v = v.wrapping_add(1));
    }
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }
    pub fn drain(&self) {
        self.draining.store(true, Ordering::SeqCst);
        self.changed();
    }
    pub fn draining(&self) -> bool {
        self.draining.load(Ordering::SeqCst)
    }
    pub fn used(&self) -> u32 {
        self.inner.lock().live.len() as u32
    }
    pub fn free(&self) -> u32 {
        if self.draining() {
            return 0;
        }
        let inner = self.inner.lock();
        inner
            .free
            .iter()
            .filter_map(Result::ok)
            .filter(|(key, _)| matches!(inner.live.contains_key(key), Ok(false)))
            .count() as u32
    }
    pub fn acquire(&self, id: &str, now: i64) -> Result<bool, String> {
        let inner = self.inner.lock();
        for item in inner.live.iter() {
            let (_, bytes) = item.map_err(|e| e.to_string())?;
            let route: Route = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if route.call_id == id {
                return if route.expiring {
                    Err("session expiring".into())
                } else {
                    Ok(false)
                };
            }
        }
        if self.draining() {
            return Err("cell draining".into());
        }
        for item in inner.free.iter() {
            let (key, _) = item.map_err(|e| e.to_string())?;
            // Live routes always override an inconsistent free-set entry.
            if inner.live.contains_key(&key).map_err(|e| e.to_string())? {
                continue;
            }
            let bytes = serde_json::to_vec(&Route {
                call_id: id.into(),
                registered_at: now,
                last_heartbeat: now,
                expiring: false,
            })
            .unwrap();
            let result: Result<(), sled::transaction::TransactionError<()>> =
                (&inner.free, &inner.live).transaction(|(free, live)| {
                    if live.get(&key)?.is_some() || free.get(&key)?.is_none() {
                        return Err(sled::transaction::ConflictableTransactionError::Abort(()));
                    }
                    live.insert(key.as_ref(), bytes.as_slice())?;
                    free.remove(key.as_ref())?;
                    Ok(())
                });
            result.map_err(|e| format!("slot transaction: {e:?}"))?;
            drop(inner);
            self.changed();
            return Ok(true);
        }
        Err("cell full".into())
    }
    pub fn touch(&self, id: &str, now: i64) -> sled::Result<()> {
        let inner = self.inner.lock();
        for item in inner.live.iter() {
            let (key, bytes) = item?;
            let mut route: Route = serde_json::from_slice(&bytes)
                .map_err(|e| sled::Error::Unsupported(e.to_string()))?;
            if route.call_id == id && !route.expiring && now > route.last_heartbeat {
                route.last_heartbeat = now;
                inner
                    .live
                    .insert(key, serde_json::to_vec(&route).unwrap())?;
                break;
            }
        }
        Ok(())
    }
    pub fn release(&self, id: &str) -> sled::Result<()> {
        let inner = self.inner.lock();
        for item in inner.live.iter() {
            let (key, bytes) = item?;
            let route: Route = serde_json::from_slice(&bytes)
                .map_err(|e| sled::Error::Unsupported(e.to_string()))?;
            if route.call_id != id {
                continue;
            }
            let result: Result<(), sled::transaction::TransactionError<()>> =
                (&inner.free, &inner.live).transaction(|(free, live)| {
                    live.remove(key.as_ref())?;
                    free.insert(key.as_ref(), &[])?;
                    Ok(())
                });
            result.map_err(|e| sled::Error::Unsupported(format!("release: {e:?}")))?;
            drop(inner);
            self.changed();
            return Ok(());
        }
        Ok(())
    }
    fn routes(&self) -> sled::Result<Vec<Route>> {
        self.inner
            .lock()
            .live
            .iter()
            .map(|item| {
                let (_, bytes) = item?;
                serde_json::from_slice(&bytes).map_err(|e| sled::Error::Unsupported(e.to_string()))
            })
            .collect()
    }

    fn mark_expiring(&self, id: &str, registered_at: i64) -> sled::Result<bool> {
        let inner = self.inner.lock();
        for item in inner.live.iter() {
            let (key, bytes) = item?;
            let mut route: Route = serde_json::from_slice(&bytes)
                .map_err(|e| sled::Error::Unsupported(e.to_string()))?;
            if route.call_id == id && route.registered_at == registered_at {
                route.expiring = true;
                inner
                    .live
                    .insert(key, serde_json::to_vec(&route).unwrap())?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Fixed lifetime measured from registration; media activity cannot extend it.
    /// Cancellation is retried while the actor shuts down. Its cleanup atomically
    /// releases the Sled slot and wakes every scheduler's capacity publisher.
    pub async fn run_reaper(&self, queue: tokio::sync::mpsc::Sender<crate::models::QueueEvent>) {
        self.run_reaper_with_lifetime(queue, std::time::Duration::from_secs(25 * 60))
            .await;
    }

    async fn run_reaper_with_lifetime(
        &self,
        queue: tokio::sync::mpsc::Sender<crate::models::QueueEvent>,
        lifetime: std::time::Duration,
    ) {
        use futures_util::StreamExt;
        use std::collections::{HashMap, HashSet};
        use tokio_util::time::{delay_queue::Key, DelayQueue};
        let mut changes = self.subscribe();
        let mut timers = DelayQueue::<(String, i64)>::new();
        let mut keys = HashMap::<(String, i64), Key>::new();
        // Watch notifications coalesce; rebuild the timer index from authoritative
        // Sled state, so no acquire/release notification can be lost.
        loop {
            changes.borrow_and_update();
            match self.routes() {
                Ok(routes) => {
                    let live: HashSet<_> = routes
                        .iter()
                        .map(|r| (r.call_id.clone(), r.registered_at))
                        .collect();
                    keys.retain(|identity, key| {
                        if live.contains(identity) {
                            true
                        } else {
                            timers.remove(key);
                            false
                        }
                    });
                    for route in routes {
                        let identity = (route.call_id, route.registered_at);
                        keys.entry(identity.clone()).or_insert_with(|| {
                            let deadline_ms = route
                                .registered_at
                                .saturating_mul(1000)
                                .saturating_add(lifetime.as_millis().min(i64::MAX as u128) as i64);
                            let remaining = deadline_ms
                                .saturating_sub(chrono::Utc::now().timestamp_millis())
                                .max(0) as u64;
                            timers.insert(identity, std::time::Duration::from_millis(remaining))
                        });
                    }
                }
                Err(error) => {
                    tracing::error!(%error, "cannot read slot deadlines; retaining existing timers")
                }
            }
            tokio::select! {
                _ = queue.closed() => return,
                result = changes.changed() => { if result.is_err() { return; } }
                expired = timers.next(), if !timers.is_empty() => {
                    if let Some(expired) = expired {
                        let identity = expired.into_inner();
                        keys.remove(&identity);
                        match self.mark_expiring(&identity.0, identity.1) {
                            Ok(true) => {
                                if let Err(error) = queue.try_send(crate::models::QueueEvent::SessionCancelled { session_id: identity.0.clone() }) {
                                    tracing::warn!(%error, session_id = %identity.0, "expiry cancellation will retry");
                                }
                                // Keep capacity occupied until actor cleanup calls release.
                                let key = timers.insert(identity.clone(), std::time::Duration::from_secs(1));
                                keys.insert(identity, key);
                            }
                            Ok(false) => {},
                            Err(error) => {
                                tracing::error!(%error, "cannot mark expired slot; retrying");
                                let key = timers.insert(identity.clone(), std::time::Duration::from_secs(1));
                                keys.insert(identity, key);
                            }
                        }
                    }
                }
                // Retry a failed storage snapshot without scanning healthy state periodically.
                _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn slots(n: u32) -> Slots {
        Slots::new(&sled::Config::new().temporary(true).open().unwrap(), n).unwrap()
    }
    #[test]
    fn concurrent_admission_never_exceeds_capacity() {
        let slots = slots(2);
        let threads: Vec<_> = (0..32)
            .map(|i| {
                let s = slots.clone();
                std::thread::spawn(move || s.acquire(&i.to_string(), 0).is_ok())
            })
            .collect();
        assert_eq!(
            threads
                .into_iter()
                .map(|t| t.join().unwrap() as u32)
                .sum::<u32>(),
            2
        );
        assert_eq!(slots.used(), 2);
    }
    #[test]
    fn duplicate_release_and_registration_deadline() {
        let s = slots(1);
        assert_eq!(s.acquire("a", 0), Ok(true));
        assert_eq!(s.acquire("a", 0), Ok(false));
        s.touch("a", 1499).unwrap();
        assert_eq!(s.routes().unwrap()[0].registered_at, 0);
        assert!(s.mark_expiring("a", 0).unwrap());
        assert!(s.acquire("b", 3000).is_err());
        s.release("a").unwrap();
        s.release("a").unwrap();
        assert_eq!(s.free(), 1);
        assert!(s.acquire("b", 3001).is_ok());
        s.drain();
        assert_eq!(s.free(), 0);
    }
    #[test]
    fn live_route_overrides_free_pool() {
        let s = slots(1);
        s.acquire("a", 0).unwrap();
        s.inner.lock().free.insert(0u32.to_be_bytes(), &[]).unwrap();
        assert!(s.acquire("b", 0).is_err());
    }
}

#[cfg(test)]
mod reaper_tests {
    use super::*;
    use crate::models::QueueEvent;
    use std::time::Duration;
    use tokio::sync::mpsc;

    fn slots() -> Slots {
        Slots::new(&sled::Config::new().temporary(true).open().unwrap(), 1).unwrap()
    }

    #[tokio::test]
    async fn registration_expiry_ignores_activity_and_release_notifies_publishers() {
        let slots = slots();
        let now = chrono::Utc::now().timestamp();
        slots.acquire("expired", now - 1501).unwrap();
        slots.touch("expired", now).unwrap();
        let mut updates = slots.subscribe();
        let (tx, mut rx) = mpsc::channel(4);
        let worker_slots = slots.clone();
        let worker = tokio::spawn(async move { worker_slots.run_reaper(tx).await });
        let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(event, QueueEvent::SessionCancelled { session_id } if session_id == "expired")
        );
        assert_eq!(slots.free(), 0, "actor must stop before slot reuse");
        assert!(slots.acquire("expired", now).is_err());
        slots.release("expired").unwrap();
        tokio::time::timeout(Duration::from_secs(1), updates.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(slots.free(), 1);
        assert!(slots.routes().unwrap().is_empty());
        assert!(tokio::time::timeout(Duration::from_millis(1100), rx.recv())
            .await
            .is_err());
        worker.abort();
    }

    #[tokio::test]
    async fn empty_delayqueue_wakes_for_new_registration_and_retries_full_queue() {
        let slots = slots();
        let (tx, mut rx) = mpsc::channel(1);
        tx.send(QueueEvent::SessionCancelled {
            session_id: "busy".into(),
        })
        .await
        .unwrap();
        let worker_slots = slots.clone();
        let worker = tokio::spawn(async move { worker_slots.run_reaper(tx).await });
        tokio::task::yield_now().await;
        slots
            .acquire("late", chrono::Utc::now().timestamp() - 1501)
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if slots.routes().unwrap()[0].expiring {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(
            matches!(rx.recv().await, Some(QueueEvent::SessionCancelled { session_id }) if session_id == "busy")
        );
        assert!(
            matches!(tokio::time::timeout(Duration::from_secs(2), rx.recv()).await.unwrap(), Some(QueueEvent::SessionCancelled { session_id }) if session_id == "late")
        );
        slots.release("late").unwrap();
        worker.abort();
    }

    #[tokio::test]
    async fn normal_release_removes_timer_and_future_deadline_does_not_fire_early() {
        let slots = slots();
        let (tx, mut rx) = mpsc::channel(4);
        let worker_slots = slots.clone();
        let worker = tokio::spawn(async move {
            worker_slots
                .run_reaper_with_lifetime(tx, Duration::from_secs(1))
                .await;
        });
        let now = chrono::Utc::now().timestamp();
        slots.acquire("finished", now + 2).unwrap();
        tokio::task::yield_now().await;
        slots.release("finished").unwrap();
        slots.acquire("next", now + 2).unwrap();
        assert!(tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .is_err());
        let event = tokio::time::timeout(Duration::from_secs(4), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(event, QueueEvent::SessionCancelled { session_id } if session_id == "next")
        );
        worker.abort();
    }
}
