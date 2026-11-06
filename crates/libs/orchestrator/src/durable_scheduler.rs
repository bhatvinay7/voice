use crate::models::{RunId, TaskId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DurableTaskStatus {
    Pending,
    Claimed,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DurableTask {
    pub task_id: TaskId,
    pub session_id: String,
    pub run_id: RunId,
    pub tool_name: String,
    pub args: serde_json::Value,
    #[serde(default)]
    pub query_node_id: Option<String>,
    #[serde(default)]
    pub query_graph_version: u64,
    pub attempt: u32,
    pub max_attempts: u32,
    pub retry_count: u32,
    pub due_at_ms: i64,
    pub lease_expires_at_ms: Option<i64>,
    pub status: DurableTaskStatus,
    pub last_error: Option<String>,
}

#[derive(Clone)]
pub struct DurableTaskScheduler {
    tasks: sled::Tree,
    due: sled::Tree,
}

impl DurableTaskScheduler {
    pub fn open(db: &sled::Db) -> Result<Self, String> {
        Ok(Self {
            tasks: db.open_tree("durable_tasks").map_err(|e| e.to_string())?,
            due: db
                .open_tree("durable_task_due")
                .map_err(|e| e.to_string())?,
        })
    }

    fn due_key(due_at_ms: i64, task_id: &TaskId) -> String {
        format!("{:020}:{}", due_at_ms.max(0), task_id.0)
    }

    fn encode(task: &DurableTask) -> Result<Vec<u8>, String> {
        serde_json::to_vec(task).map_err(|e| e.to_string())
    }

    fn decode(bytes: &[u8]) -> Result<DurableTask, String> {
        serde_json::from_slice(bytes).map_err(|e| e.to_string())
    }

    fn remove_due_entries(&self, task_id: &TaskId) -> Result<(), String> {
        let entries = self
            .due
            .iter()
            .filter_map(Result::ok)
            .filter(|(_, value)| value.as_ref() == task_id.0.as_bytes())
            .map(|(key, _)| key)
            .collect::<Vec<_>>();
        for key in entries {
            self.due.remove(key).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn schedule(&self, task: &DurableTask) -> Result<(), String> {
        self.remove_due_entries(&task.task_id)?;
        self.tasks
            .insert(task.task_id.0.as_bytes(), Self::encode(task)?)
            .map_err(|e| e.to_string())?;
        if task.status == DurableTaskStatus::Pending {
            self.due
                .insert(
                    Self::due_key(task.due_at_ms, &task.task_id).as_bytes(),
                    task.task_id.0.as_bytes(),
                )
                .map_err(|e| e.to_string())?;
        }
        self.tasks.flush().map_err(|e| e.to_string())?;
        self.due.flush().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn load(&self, task_id: &TaskId) -> Result<Option<DurableTask>, String> {
        self.tasks
            .get(task_id.0.as_bytes())
            .map_err(|e| e.to_string())?
            .map(|bytes| Self::decode(&bytes))
            .transpose()
    }

    /// Compare-and-swap makes claiming safe when two local workers observe the
    /// same due key. Stale due keys are harmless because only one claim wins.
    pub fn claim_due(
        &self,
        now_ms: i64,
        lease_ms: u64,
        limit: usize,
    ) -> Result<Vec<DurableTask>, String> {
        let mut claimed = Vec::new();
        for item in self.due.iter() {
            if claimed.len() >= limit {
                break;
            }
            let (key, value) = item.map_err(|e| e.to_string())?;
            let key_str = std::str::from_utf8(&key).map_err(|e| e.to_string())?;
            let Some((due, _)) = key_str.split_once(':') else {
                continue;
            };
            let due_at = due.parse::<i64>().unwrap_or(i64::MAX);
            if due_at > now_ms {
                break;
            }
            let task_id = TaskId(String::from_utf8(value.to_vec()).map_err(|e| e.to_string())?);
            let Some(current) = self
                .tasks
                .get(task_id.0.as_bytes())
                .map_err(|e| e.to_string())?
            else {
                let _ = self.due.remove(key);
                continue;
            };
            let mut task = Self::decode(&current)?;
            if task.status != DurableTaskStatus::Pending || task.due_at_ms > now_ms {
                let _ = self.due.remove(key);
                continue;
            }
            task.status = DurableTaskStatus::Claimed;
            task.lease_expires_at_ms = Some(now_ms.saturating_add(lease_ms as i64));
            let updated = Self::encode(&task)?;
            match self
                .tasks
                .compare_and_swap(task_id.0.as_bytes(), Some(current), Some(updated))
                .map_err(|e| e.to_string())?
            {
                Ok(()) => {
                    let _ = self.due.remove(key);
                    claimed.push(task);
                }
                Err(_) => continue,
            }
        }
        Ok(claimed)
    }

    pub fn recover_expired_claims(&self, now_ms: i64) -> Result<(), String> {
        for item in self.tasks.iter() {
            let (key, value) = item.map_err(|e| e.to_string())?;
            let mut task = Self::decode(&value)?;
            if task.status == DurableTaskStatus::Claimed
                && task
                    .lease_expires_at_ms
                    .is_some_and(|lease| lease <= now_ms)
            {
                task.status = DurableTaskStatus::Pending;
                task.due_at_ms = now_ms;
                task.lease_expires_at_ms = None;
                self.tasks
                    .insert(key, Self::encode(&task)?)
                    .map_err(|e| e.to_string())?;
                self.due
                    .insert(
                        Self::due_key(now_ms, &task.task_id).as_bytes(),
                        task.task_id.0.as_bytes(),
                    )
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    pub fn renew_lease(
        &self,
        task_id: &TaskId,
        now_ms: i64,
        lease_ms: u64,
    ) -> Result<bool, String> {
        let Some(mut task) = self.load(task_id)? else {
            return Ok(false);
        };
        if task.status != DurableTaskStatus::Claimed {
            return Ok(false);
        }
        task.lease_expires_at_ms = Some(now_ms.saturating_add(lease_ms as i64));
        self.tasks
            .insert(task_id.0.as_bytes(), Self::encode(&task)?)
            .map_err(|e| e.to_string())?;
        self.tasks.flush().map_err(|e| e.to_string())?;
        Ok(true)
    }

    pub fn terminal(
        &self,
        task_id: &TaskId,
        status: DurableTaskStatus,
        error: Option<String>,
    ) -> Result<(), String> {
        let Some(mut task) = self.load(task_id)? else {
            return Ok(());
        };
        task.status = status;
        task.last_error = error;
        task.lease_expires_at_ms = None;
        self.schedule(&task)
    }
}
