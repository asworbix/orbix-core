use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::cmp::Ordering;
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Job Definition
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub job_type: String,
    pub payload: serde_json::Value,
    pub priority: u8,           // 0–9, default 5
    pub max_retries: u32,       // default 3
    pub timeout_ms: u64,        // default 60000
    pub schedule_at: Option<String>,  // ISO 8601 future scheduling
    pub status: JobStatus,
    pub attempt: u32,
    pub result: Option<JobResult>,
    pub error: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Retrying,
    Dead,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobResult {
    pub success: bool,
    pub output: serde_json::Value,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobInput {
    pub job_type: String,
    pub payload: serde_json::Value,
    pub priority: Option<u8>,
    pub max_retries: Option<u32>,
    pub timeout_ms: Option<u64>,
    pub schedule_at: Option<String>,
}

// ─────────────────────────────────────────────
//  Retry Policy
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RetryPolicy {
    FixedDelay { delay_ms: u64 },
    ExponentialBackoff { base_ms: u64, max_ms: u64 },
}

impl RetryPolicy {
    pub fn delay_for_attempt(&self, attempt: u32) -> u64 {
        match self {
            RetryPolicy::FixedDelay { delay_ms } => *delay_ms,
            RetryPolicy::ExponentialBackoff { base_ms, max_ms } => {
                let delay = base_ms * 2u64.pow(attempt.saturating_sub(1));
                delay.min(*max_ms)
            }
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy::ExponentialBackoff { base_ms: 1000, max_ms: 30000 }
    }
}

// ─────────────────────────────────────────────
//  Priority Wrapper for BinaryHeap
// ─────────────────────────────────────────────

#[derive(Clone, Debug)]
struct PrioritizedJob {
    priority: u8,
    created_at: String,
    job_id: String,
}

impl PartialEq for PrioritizedJob {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.job_id == other.job_id
    }
}
impl Eq for PrioritizedJob {}

impl PartialOrd for PrioritizedJob {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrioritizedJob {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority first, then earlier creation time
        self.priority.cmp(&other.priority)
            .then_with(|| other.created_at.cmp(&self.created_at))
    }
}

// ─────────────────────────────────────────────
//  Job Scheduler
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SchedulerStats {
    pub total_submitted: u64,
    pub total_completed: u64,
    pub total_failed: u64,
    pub total_dead: u64,
    pub queue_depth: usize,
    pub running_count: usize,
    pub dead_letter_count: usize,
}

pub struct JobScheduler {
    jobs: HashMap<String, Job>,
    queue: BinaryHeap<PrioritizedJob>,
    dead_letter: VecDeque<Job>,
    retry_policy: RetryPolicy,
    max_queue_size: usize,
    dead_letter_max: usize,
    stats: SchedulerStats,
}

impl JobScheduler {
    pub fn new(max_queue_size: usize, dead_letter_max: usize) -> Self {
        Self {
            jobs: HashMap::new(),
            queue: BinaryHeap::new(),
            dead_letter: VecDeque::new(),
            retry_policy: RetryPolicy::default(),
            max_queue_size,
            dead_letter_max,
            stats: SchedulerStats {
                total_submitted: 0,
                total_completed: 0,
                total_failed: 0,
                total_dead: 0,
                queue_depth: 0,
                running_count: 0,
                dead_letter_count: 0,
            },
        }
    }

    pub fn set_retry_policy(&mut self, policy: RetryPolicy) {
        self.retry_policy = policy;
    }

    /// Submit a new job. Returns the job or error if queue is full.
    pub fn submit(&mut self, input: JobInput) -> Result<Job, String> {
        if self.queue.len() >= self.max_queue_size {
            return Err("Job queue is full".into());
        }

        let job = Job {
            id: Uuid::new_v4().to_string(),
            job_type: input.job_type,
            payload: input.payload,
            priority: input.priority.unwrap_or(5).min(9),
            max_retries: input.max_retries.unwrap_or(3),
            timeout_ms: input.timeout_ms.unwrap_or(60000),
            schedule_at: input.schedule_at,
            status: JobStatus::Pending,
            attempt: 0,
            result: None,
            error: None,
            created_at: Utc::now().to_rfc3339(),
            started_at: None,
            completed_at: None,
        };

        let pj = PrioritizedJob {
            priority: job.priority,
            created_at: job.created_at.clone(),
            job_id: job.id.clone(),
        };

        self.jobs.insert(job.id.clone(), job.clone());
        self.queue.push(pj);
        self.stats.total_submitted += 1;
        self.update_stats();
        Ok(job)
    }

    /// Dequeue the highest priority job ready for execution.
    pub fn dequeue(&mut self) -> Option<Job> {
        // Jobs scheduled for the future are popped but must be put back without
        // being re-popped in this same call, otherwise a queue containing only
        // future jobs spins forever. Collect skipped jobs and re-push after.
        let mut deferred: Vec<PrioritizedJob> = Vec::new();
        let result = loop {
            let Some(pj) = self.queue.pop() else { break None };
            if let Some(job) = self.jobs.get_mut(&pj.job_id) {
                if job.status == JobStatus::Pending || job.status == JobStatus::Retrying {
                    // Check if scheduled for the future
                    if let Some(ref sched) = job.schedule_at {
                        if sched.as_str() > Utc::now().to_rfc3339().as_str() {
                            deferred.push(pj);
                            continue;
                        }
                    }
                    job.status = JobStatus::Running;
                    job.started_at = Some(Utc::now().to_rfc3339());
                    job.attempt += 1;
                    let r = job.clone();
                    break Some(r);
                }
            }
        };
        for pj in deferred {
            self.queue.push(pj);
        }
        self.update_stats();
        result
    }

    /// Mark a job as completed with a result.
    pub fn complete(&mut self, job_id: &str, output: serde_json::Value, duration_ms: u64) -> Option<Job> {
        let result = {
            let job = self.jobs.get_mut(job_id)?;
            job.status = JobStatus::Completed;
            job.completed_at = Some(Utc::now().to_rfc3339());
            job.result = Some(JobResult { success: true, output, duration_ms });
            self.stats.total_completed += 1;
            job.clone()
        };
        self.update_stats();
        Some(result)
    }

    /// Mark a job as failed. Will retry if attempts remain, otherwise moves to dead-letter.
    pub fn fail(&mut self, job_id: &str, error: String) -> Option<Job> {
        let (result, enqueue, dead) = {
            let job = self.jobs.get_mut(job_id)?;
            if job.attempt < job.max_retries {
                job.status = JobStatus::Retrying;
                job.error = Some(error);
                let pj = PrioritizedJob {
                    priority: job.priority,
                    created_at: job.created_at.clone(),
                    job_id: job.id.clone(),
                };
                self.stats.total_failed += 1;
                (job.clone(), Some(pj), None)
            } else {
                job.status = JobStatus::Dead;
                job.error = Some(error);
                job.completed_at = Some(Utc::now().to_rfc3339());
                self.stats.total_dead += 1;
                (job.clone(), None, Some(job.clone()))
            }
        };
        if let Some(pj) = enqueue {
            self.queue.push(pj);
        }
        if let Some(dead_job) = dead {
            self.dead_letter.push_back(dead_job);
            if self.dead_letter.len() > self.dead_letter_max {
                self.dead_letter.pop_front();
            }
        }
        self.update_stats();
        Some(result)
    }

    /// Cancel a pending job.
    pub fn cancel(&mut self, job_id: &str) -> Option<Job> {
        let result = {
            let job = self.jobs.get_mut(job_id)?;
            if job.status != JobStatus::Pending && job.status != JobStatus::Retrying {
                return None;
            }
            job.status = JobStatus::Cancelled;
            job.completed_at = Some(Utc::now().to_rfc3339());
            job.clone()
        };
        self.update_stats();
        Some(result)
    }

    /// Get a job by ID.
    pub fn get(&self, job_id: &str) -> Option<&Job> {
        self.jobs.get(job_id)
    }

    /// List jobs with optional status filter.
    pub fn list(&self, status: Option<&JobStatus>, limit: usize) -> Vec<&Job> {
        self.jobs.values()
            .filter(|j| status.map_or(true, |s| &j.status == s))
            .take(limit)
            .collect()
    }

    /// Get dead-letter queue contents.
    pub fn dead_letter_queue(&self) -> Vec<&Job> {
        self.dead_letter.iter().collect()
    }

    /// Get scheduler statistics.
    pub fn stats(&self) -> &SchedulerStats {
        &self.stats
    }

    /// Retry delay for a job's current attempt.
    pub fn retry_delay(&self, job: &Job) -> u64 {
        self.retry_policy.delay_for_attempt(job.attempt)
    }

    fn update_stats(&mut self) {
        self.stats.queue_depth = self.queue.len();
        self.stats.running_count = self.jobs.values().filter(|j| j.status == JobStatus::Running).count();
        self.stats.dead_letter_count = self.dead_letter.len();
    }

    /// Purge completed/dead jobs older than retention hours.
    pub fn purge(&mut self, retention_hours: u64) {
        let cutoff = chrono::Utc::now() - chrono::Duration::hours(retention_hours as i64);
        let cutoff_str = cutoff.to_rfc3339();
        self.jobs.retain(|_, j| {
            match j.status {
                JobStatus::Completed | JobStatus::Dead | JobStatus::Cancelled => {
                    j.completed_at.as_ref().map_or(true, |t| t.as_str() > cutoff_str.as_str())
                }
                _ => true,
            }
        });
    }
}

impl Default for JobScheduler {
    fn default() -> Self {
        Self::new(10000, 1000)
    }
}
