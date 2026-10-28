//! Session-owned phone output. No LiveKit room or browser broadcast is involved.
use dashmap::{mapref::entry::Entry, DashMap};
use parking_lot::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub enum PhoneOutput {
    Audio { pcm: Vec<i16>, sample_rate: u32 },
    Clear,
}

struct Session {
    tx: mpsc::Sender<PhoneOutput>,
    rx: Mutex<Option<mpsc::Receiver<PhoneOutput>>>,
    attached: AtomicBool,
    cancelled: CancellationToken,
}

#[derive(Clone, Default)]
pub struct PhoneMediaRegistry {
    sessions: Arc<DashMap<String, Arc<Session>>>,
}

impl PhoneMediaRegistry {
    /// Reserve before admitting the call, making webhook retries idempotent.
    pub fn reserve(&self, id: &str) -> bool {
        match self.sessions.entry(id.to_owned()) {
            Entry::Occupied(_) => false,
            Entry::Vacant(entry) => {
                let (tx, rx) = mpsc::channel(100);
                entry.insert(Arc::new(Session {
                    tx,
                    rx: Mutex::new(Some(rx)),
                    attached: AtomicBool::new(false),
                    cancelled: CancellationToken::new(),
                }));
                true
            }
        }
    }

    pub fn sender(&self, id: &str) -> Option<mpsc::Sender<PhoneOutput>> {
        self.sessions.get(id).map(|session| session.tx.clone())
    }

    pub fn attach(
        &self,
        id: &str,
    ) -> Result<(mpsc::Receiver<PhoneOutput>, CancellationToken), String> {
        let session = self.sessions.get(id).ok_or("Phone session is not active")?;
        let rx = session
            .rx
            .lock()
            .take()
            .ok_or("A phone socket is already attached")?;
        session.attached.store(true, Ordering::SeqCst);
        Ok((rx, session.cancelled.clone()))
    }

    pub fn is_waiting(&self, id: &str) -> bool {
        self.sessions
            .get(id)
            .is_some_and(|session| !session.attached.load(Ordering::SeqCst))
    }

    pub fn close(&self, id: &str) {
        if let Some((_, session)) = self.sessions.remove(id) {
            session.cancelled.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn sockets_are_exclusive_and_calls_are_isolated() {
        let registry = PhoneMediaRegistry::default();
        assert!(registry.reserve("phone:a"));
        assert!(!registry.reserve("phone:a"));
        registry.reserve("phone:b");
        let (mut a, cancelled) = registry.attach("phone:a").unwrap();
        let (mut b, _) = registry.attach("phone:b").unwrap();
        assert!(registry.attach("phone:a").is_err());
        registry
            .sender("phone:a")
            .unwrap()
            .send(PhoneOutput::Clear)
            .await
            .unwrap();
        assert!(matches!(a.recv().await, Some(PhoneOutput::Clear)));
        assert!(b.try_recv().is_err());
        registry.close("phone:a");
        assert!(cancelled.is_cancelled());
        assert!(registry.sender("phone:a").is_none());
        assert!(registry.sender("phone:b").is_some());
    }
}
