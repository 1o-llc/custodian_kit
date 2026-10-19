//! A task's state, one word, derived from the records each time it is read:
//! never stored as a second copy that could drift from them.
//!
//! - `proposed`: a plan or a change spec no preflight has authorized.
//! - `ready`: authorized by a preflight, or a job folder queued to run.
//! - `running`: a job folder in `jobs/running` with nothing open.
//! - `blocked`: a running job with a decision open.
//! - `paused`: a queued job with a checkpoint (interrupted, resumes at its close).
//! - `verifying`: closed `pending`: its verification is not declared or not satisfied yet.
//! - `completed`: closed `accepted`.
//! - `failed`: closed `failed_test` or `rejected`.

use big_bad_bool::bitpack;

/// The states a task is in.
#[bitpack(storage = u8, version = 1)]
pub struct TaskState {
    #[bits(3)]
    #[describe("where a task stands, from its records")]
    #[labels(proposed = 0, ready = 1, running = 2, blocked = 3, paused = 4, verifying = 5, completed = 6, failed = 7)]
    state: u8,
}

impl TaskState {
    /// Every state's word, in order.
    pub fn words() -> Vec<&'static str> {
        Self::LABELS.iter().filter(|l| l.field == "state").map(|l| l.label).collect()
    }

    /// The state a word names.
    pub fn of_word(word: &str) -> Option<TaskState> {
        Self::LABELS.iter().find(|l| l.field == "state" && l.label == word).map(|l| Self::from_raw(l.value as u8))
    }

    pub fn word(self) -> &'static str {
        Self::LABELS.iter().find(|l| l.field == "state" && l.value == self.state() as u128).map(|l| l.label).unwrap_or("proposed")
    }

    /// A job's state: its folder's state, its task's result, whether a decision of its task is open, whether it was
    /// checkpointed in the queue.
    pub fn of_job(folder_state: &str, result: Option<&str>, decisions_open: bool, checkpointed: bool) -> TaskState {
        let w = match (folder_state, result) {
            ("done", Some("accepted")) => "completed",
            ("done", Some("failed_test" | "rejected")) => "failed",
            ("done", _) => "verifying",
            ("running", _) if decisions_open => "blocked",
            ("running", _) => "running",
            ("queued", _) if checkpointed => "paused",
            ("queued", _) => "ready",
            _ => "proposed",
        };
        Self::of_word(w).unwrap_or(Self::from_raw(0))
    }

    /// A change spec's state: authorized by a preflight of its current version, or not yet.
    pub fn of_spec(authorized: bool) -> TaskState {
        Self::of_word(if authorized { "ready" } else { "proposed" }).unwrap_or(Self::from_raw(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_states_are_the_specification_s_words() {
        assert_eq!(TaskState::words(), ["proposed", "ready", "running", "blocked", "paused", "verifying", "completed", "failed"]);
        assert_eq!(TaskState::of_job("done", Some("accepted"), false, false).word(), "completed");
        assert_eq!(TaskState::of_job("done", Some("rejected"), false, false).word(), "failed");
        assert_eq!(TaskState::of_job("done", Some("pending"), false, false).word(), "verifying");
        assert_eq!(TaskState::of_job("running", None, true, false).word(), "blocked");
        assert_eq!(TaskState::of_job("queued", None, false, true).word(), "paused");
        assert_eq!(TaskState::of_spec(false).word(), "proposed");
    }
}
