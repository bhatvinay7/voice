use crate::models::TaskId;
use std::collections::HashMap;
use tokio_util::time::delay_queue::{DelayQueue, Key};

pub struct TimerWheel {
    pub queue: DelayQueue<TaskId>,
    pub keys: HashMap<TaskId, Key>,
}

impl TimerWheel {
    pub fn new() -> Self {
        Self {
            queue: DelayQueue::new(),
            keys: HashMap::new(),
        }
    }
}
