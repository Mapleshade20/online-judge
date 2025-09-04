use std::collections::VecDeque;

use std::sync::Mutex;
use tokio::sync::Notify;

use crate::routes::JobMessage;

#[derive(Default)]
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

    pub fn push(&self, job: JobMessage) {
        self.queue.lock().unwrap().push_back(job);
        self.notify.notify_one();
    }

    pub async fn pop(&self) -> JobMessage {
        loop {
            if let Some(job) = self.queue.lock().unwrap().pop_front() {
                return job;
            }
            self.notify.notified().await;
        }
    }

    pub fn cancel_job(&self, job_id: u32) -> bool {
        let mut queue = self.queue.lock().unwrap();
        let before_len = queue.len();
        queue.retain(|j| j.id() != job_id);
        before_len != queue.len()
    }
}
