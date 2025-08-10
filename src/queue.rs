use std::collections::VecDeque;

use tokio::sync::{Mutex, Notify};

use crate::routes::JobMessage;

pub struct JobQueue {
    queue: Mutex<VecDeque<JobMessage>>,
    notify: Notify,
}

impl JobQueue {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
        }
    }

    pub async fn push(&self, job: JobMessage) {
        self.queue.lock().await.push_back(job);
        self.notify.notify_one();
    }

    pub async fn pop(&self) -> JobMessage {
        loop {
            if let Some(job) = self.queue.lock().await.pop_front() {
                return job;
            }
            self.notify.notified().await;
        }
    }

    pub async fn cancel_job(&self, job_id: u32) -> bool {
        let mut queue = self.queue.lock().await;
        let before_len = queue.len();
        queue.retain(|j| j.id() != job_id);
        before_len != queue.len()
    }
}
