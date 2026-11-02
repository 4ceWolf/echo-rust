//! Queue state: shuffle/repeat/history logic, ported design-for-design
//! from the Python `player.py` that was never a source of a single bug
//! across this whole project's history (every prior bug was in UI
//! wiring, not here) - see docs/ARCHITECTURE_DECISIONS.md. This is a
//! faithful port of that logic into Rust, not new design.
//!
//! Deliberately does NOT include 8D audio - that feature was explicitly
//! removed by user decision earlier in this project (mono/concert-hall/
//! speed survived that removal, 8D specifically did not). If you're
//! reading old Python source with 8D still in it, that's expected - the
//! upload predates the removal decision but this rewrite honors it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::RngExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RepeatMode {
    Off,
    One,
    All,
}

impl Default for RepeatMode {
    fn default() -> Self {
        RepeatMode::Off
    }
}

impl RepeatMode {
    /// Matches the Python version's cycle order exactly: off -> all -> one -> off.
    pub fn cycle(self) -> RepeatMode {
        match self {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        }
    }
}

pub struct QueueState {
    pub queue: Vec<PathBuf>,
    pub queue_index: usize,
    pub history: Vec<usize>,
    pub shuffle: bool,
    pub smart_shuffle: bool,
    /// Bias strength for smart shuffle: 0.0 = uniform random, 1.0 = strongly
    /// favor tracks not played recently (see `weighted_choice`).
    pub smart_shuffle_bias: f64,
    pub repeat: RepeatMode,
}

impl QueueState {
    pub fn new() -> Self {
        Self {
            queue: Vec::new(),
            queue_index: 0,
            history: Vec::new(),
            shuffle: false,
            smart_shuffle: false,
            smart_shuffle_bias: 1.0,
            repeat: RepeatMode::Off,
        }
    }

    pub fn set_queue(&mut self, paths: Vec<PathBuf>, start_index: usize) {
        self.queue = paths;
        self.queue_index = start_index.min(self.queue.len().saturating_sub(1));
        self.history.clear();
    }

    /// Mutates queue_index/history to whatever should play next and
    /// returns that path, or None if there's nothing left (queue empty,
    /// or end reached with repeat off). Shared by the manual "skip" path
    /// and the gapless about-to-finish path in the caller, so both
    /// commit to the same decision - important for shuffle, where "peek
    /// then re-roll" would pick two different tracks (same comment as
    /// the Python original, same reason it matters).
    pub fn decide_next(&mut self, play_history: &HashMap<String, u64>) -> Option<PathBuf> {
        if self.queue.is_empty() {
            return None;
        }
        if self.repeat == RepeatMode::One {
            return Some(self.queue[self.queue_index].clone());
        }

        self.history.push(self.queue_index);

        if self.shuffle {
            if self.queue.len() > 1 {
                let choices: Vec<usize> = (0..self.queue.len())
                    .filter(|&i| i != self.queue_index)
                    .collect();
                self.queue_index = if self.smart_shuffle {
                    weighted_choice(&choices, &self.queue, play_history, self.smart_shuffle_bias)
                } else {
                    choices[rand::rng().random_range(0..choices.len())]
                };
            }
            return Some(self.queue[self.queue_index].clone());
        }

        self.queue_index += 1;
        if self.queue_index >= self.queue.len() {
            if self.repeat == RepeatMode::All {
                self.queue_index = 0;
            } else {
                self.queue_index = self.queue.len() - 1;
                return None;
            }
        }
        Some(self.queue[self.queue_index].clone())
    }

    /// Non-mutating "what would `decide_next` return?" - used to preload the
    /// next track for gapless playback WITHOUT advancing `queue_index`
    /// (advance happens later, when the handoff actually occurs). Returns
    /// `None` when shuffle is on: shuffle's pick is random, and "peek then
    /// re-roll" would pick a different track than the real `decide_next`
    /// (see the doc comment on `decide_next`). Shuffle therefore falls back
    /// to the EOS-advance path, which is fine - a gap on shuffle is expected.
    pub fn peek_next(&self) -> Option<PathBuf> {
        if self.shuffle || self.queue.is_empty() {
            return None;
        }
        if self.repeat == RepeatMode::One {
            return Some(self.queue[self.queue_index].clone());
        }
        let mut idx = self.queue_index + 1;
        if idx >= self.queue.len() {
            if self.repeat == RepeatMode::All {
                idx = 0;
            } else {
                return None;
            }
        }
        Some(self.queue[idx].clone())
    }

    /// If more than 3s into the track, the caller should restart it
    /// instead of calling this - same rule as the Python version. This
    /// function only handles the "actually go back" half.
    pub fn decide_prev(&mut self) -> Option<PathBuf> {
        if self.queue.is_empty() {
            return None;
        }
        if self.shuffle {
            if let Some(prev_index) = self.history.pop() {
                self.queue_index = prev_index;
                return Some(self.queue[self.queue_index].clone());
            }
            return None;
        }
        if self.queue_index == 0 {
            self.queue_index = if self.repeat == RepeatMode::All {
                self.queue.len() - 1
            } else {
                0
            };
        } else {
            self.queue_index -= 1;
        }
        Some(self.queue[self.queue_index].clone())
    }
}

/// Smart shuffle: weight toward tracks not played recently instead of
/// pure random. Never-played tracks get the highest weight (10 years'
/// worth, capped) - same formula as the Python `_weighted_choice`.
fn weighted_choice(
    choices: &[usize],
    queue: &[PathBuf],
    play_history: &HashMap<String, u64>,
    bias: f64,
) -> usize {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // `bias` 0.0 -> every choice weighted equally (plain random); 1.0 -> the
    // full age weighting (never/recently-unplayed tracks dominate).
    let bias = bias.clamp(0.0, 1.0);
    let weights: Vec<f64> = choices
        .iter()
        .map(|&i| {
            let path_str = queue[i].to_string_lossy().to_string();
            let last = play_history.get(&path_str).copied().unwrap_or(0);
            let age_days = if last > 0 {
                (now.saturating_sub(last)) as f64 / 86400.0
            } else {
                3650.0 // never played -> "10 years ago", same as Python
            };
            1.0 + bias * age_days.min(3650.0)
        })
        .collect();

    let total: f64 = weights.iter().sum();
    let mut roll = rand::rng().random_range(0.0..total);
    for (idx, &w) in weights.iter().enumerate() {
        if roll < w {
            return choices[idx];
        }
        roll -= w;
    }
    choices[choices.len() - 1] // fallback for float-rounding edge case
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    fn history() -> HashMap<String, u64> {
        HashMap::new()
    }

    #[test]
    fn repeat_cycle_order_is_off_all_one() {
        assert_eq!(RepeatMode::Off.cycle(), RepeatMode::All);
        assert_eq!(RepeatMode::All.cycle(), RepeatMode::One);
        assert_eq!(RepeatMode::One.cycle(), RepeatMode::Off);
    }

    #[test]
    fn sequential_next_advances_then_stops_at_end() {
        let mut q = QueueState::new();
        q.set_queue(paths(&["a", "b", "c"]), 0);
        assert_eq!(q.decide_next(&history()).unwrap(), PathBuf::from("b"));
        assert_eq!(q.decide_next(&history()).unwrap(), PathBuf::from("c"));
        assert_eq!(q.decide_next(&history()), None); // end + repeat off
    }

    #[test]
    fn repeat_all_wraps_to_start() {
        let mut q = QueueState::new();
        q.set_queue(paths(&["a", "b"]), 1); // current = "b"
        q.repeat = RepeatMode::All;
        assert_eq!(q.decide_next(&history()).unwrap(), PathBuf::from("a"));
    }

    #[test]
    fn repeat_one_repeats_current() {
        let mut q = QueueState::new();
        q.set_queue(paths(&["a", "b"]), 0);
        q.repeat = RepeatMode::One;
        assert_eq!(q.decide_next(&history()).unwrap(), PathBuf::from("a"));
        assert_eq!(q.decide_next(&history()).unwrap(), PathBuf::from("a"));
    }

    #[test]
    fn prev_goes_back() {
        let mut q = QueueState::new();
        q.set_queue(paths(&["a", "b", "c"]), 2); // current = "c"
        assert_eq!(q.decide_prev().unwrap(), PathBuf::from("b"));
    }

    #[test]
    fn shuffle_never_picks_current() {
        let mut q = QueueState::new();
        q.set_queue(paths(&["a", "b", "c"]), 0); // current = "a"
        q.shuffle = true;
        let next = q.decide_next(&history()).unwrap();
        assert!(next == PathBuf::from("b") || next == PathBuf::from("c"));
    }

    #[test]
    fn set_queue_clamps_start_index() {
        let mut q = QueueState::new();
        q.set_queue(paths(&["a", "b"]), 99);
        assert_eq!(q.queue_index, 1); // clamped to len-1
    }

    #[test]
    fn peek_next_returns_next_without_mutating() {
        let mut q = QueueState::new();
        q.set_queue(paths(&["a", "b", "c"]), 0);
        assert_eq!(q.peek_next(), Some(PathBuf::from("b")));
        assert_eq!(q.queue_index, 0); // unchanged
        assert!(q.history.is_empty());
    }

    #[test]
    fn peek_next_none_at_end_with_repeat_off() {
        let mut q = QueueState::new();
        q.set_queue(paths(&["a", "b"]), 1); // at "b" (last)
        assert_eq!(q.peek_next(), None);
    }

    #[test]
    fn peek_next_wraps_with_repeat_all() {
        let mut q = QueueState::new();
        q.set_queue(paths(&["a", "b"]), 1); // at "b"
        q.repeat = RepeatMode::All;
        assert_eq!(q.peek_next(), Some(PathBuf::from("a")));
    }

    #[test]
    fn peek_next_none_when_shuffling() {
        let mut q = QueueState::new();
        q.set_queue(paths(&["a", "b"]), 0);
        q.shuffle = true;
        assert_eq!(q.peek_next(), None); // shuffle can't be peeked safely
    }
}
