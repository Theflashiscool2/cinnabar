//! Bounded model-failure inventory. No failed catalog is published as usable graphics.
use serde::Serialize;

const MAX_GROUPS: usize = 1024;
const MAX_SAMPLES: usize = 4;

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub checked_states: u64,
    pub failed_states: u64,
    pub omitted_failures: u64,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Serialize)]
pub struct Issue {
    pub block: String,
    pub reason: String,
    pub affected_states: u64,
    pub sample_state_ids: Vec<u32>,
}

impl Report {
    /// Counts the first compiler failure per state; independent states are still checked.
    pub fn record(&mut self, state: u32, block: &str, reason: &str) {
        self.failed_states += 1;
        self.add(Issue {
            block: block.chars().take(128).collect(),
            reason: reason.chars().take(512).collect(),
            affected_states: 1,
            sample_state_ids: vec![state],
        });
    }

    fn add(&mut self, issue: Issue) {
        if let Some(group) = self
            .issues
            .iter_mut()
            .find(|g| g.block == issue.block && g.reason == issue.reason)
        {
            group.affected_states += issue.affected_states;
            for id in issue.sample_state_ids {
                if group.sample_state_ids.len() < MAX_SAMPLES
                    && !group.sample_state_ids.contains(&id)
                {
                    group.sample_state_ids.push(id);
                }
            }
        } else if self.issues.len() < MAX_GROUPS {
            self.issues.push(issue);
        } else {
            self.omitted_failures += issue.affected_states;
        }
    }

    pub fn merge(&mut self, other: Self) {
        self.checked_states += other.checked_states;
        self.failed_states += other.failed_states;
        self.omitted_failures += other.omitted_failures;
        for issue in other.issues {
            self.add(issue);
        }
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Java model support: {} of {} states failed ({} groups).",
            self.failed_states,
            self.checked_states,
            self.issues.len()
        )?;
        for issue in self.issues.iter().take(3) {
            write!(f, " {}: {}.", issue.block, issue.reason)?;
        }
        if self.issues.len() > 3 || self.omitted_failures > 0 {
            write!(f, " Additional failures are recorded in the model audit.")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn groups_and_samples_are_bounded_without_hiding_omitted_counts() {
        let mut report = Report::default();
        for id in 0..12 {
            report.record(id, "minecraft:water", "special renderer");
        }
        assert_eq!(report.issues[0].affected_states, 12);
        assert_eq!(report.issues[0].sample_state_ids, [0, 1, 2, 3]);
        for id in 0..MAX_GROUPS as u32 {
            report.record(id, &format!("test:{id}"), "missing");
        }
        assert_eq!(report.issues.len(), MAX_GROUPS);
        assert_eq!(report.omitted_failures, 1);
        assert_eq!(report.failed_states, 12 + MAX_GROUPS as u64);
    }
}
