//! Runner-to-job matching logic.
//!
//! Determines whether a runner's capabilities satisfy a job's requirements.

use foundry_core::RunnerRequirements;

/// A runner row from the database.
#[derive(Debug, Clone)]
pub struct Runner {
    pub name: String,
    pub tags: Vec<String>,
    pub cpu: Option<i32>,
    pub memory_mb: Option<i32>,
    pub gpu: Option<i32>,
    pub arch: String,
}

/// Returns `true` if the runner satisfies all of the job's requirements.
pub fn runner_matches_requirements(runner: &Runner, requirements: &RunnerRequirements) -> bool {
    // If a specific runner name is required, match by name
    if let Some(ref name) = requirements.runner_name {
        if runner.name != *name {
            return false;
        }
    }

    // All required tags must be present in runner's tags
    for tag in &requirements.required_tags {
        if !runner.tags.contains(tag) {
            return false;
        }
    }

    // CPU check
    if let Some(min_cpu) = requirements.min_cpu {
        if runner.cpu.unwrap_or(0) < min_cpu as i32 {
            return false;
        }
    }

    // Memory check
    if let Some(min_mem) = requirements.min_memory_mb {
        if runner.memory_mb.unwrap_or(0) < min_mem as i32 {
            return false;
        }
    }

    // GPU check
    if let Some(min_gpu) = requirements.min_gpu {
        if runner.gpu.unwrap_or(0) < min_gpu as i32 {
            return false;
        }
    }

    // Architecture check
    if let Some(ref arch) = requirements.arch {
        if runner.arch != *arch {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_runner() -> Runner {
        Runner {
            name: "fast".to_string(),
            tags: vec!["linux".to_string(), "docker".to_string()],
            cpu: Some(8),
            memory_mb: Some(16384),
            gpu: Some(1),
            arch: "x86_64".to_string(),
        }
    }

    #[test]
    fn no_requirements_matches_any_runner() {
        let runner = make_runner();
        let req = RunnerRequirements::default();
        assert!(runner_matches_requirements(&runner, &req));
    }

    #[test]
    fn runner_name_match() {
        let runner = make_runner();
        let req = RunnerRequirements {
            runner_name: Some("fast".to_string()),
            ..Default::default()
        };
        assert!(runner_matches_requirements(&runner, &req));
    }

    #[test]
    fn runner_name_mismatch() {
        let runner = make_runner();
        let req = RunnerRequirements {
            runner_name: Some("slow".to_string()),
            ..Default::default()
        };
        assert!(!runner_matches_requirements(&runner, &req));
    }

    #[test]
    fn tags_subset_matches() {
        let runner = make_runner();
        let req = RunnerRequirements {
            required_tags: vec!["linux".to_string()],
            ..Default::default()
        };
        assert!(runner_matches_requirements(&runner, &req));
    }

    #[test]
    fn tags_missing_fails() {
        let runner = make_runner();
        let req = RunnerRequirements {
            required_tags: vec!["windows".to_string()],
            ..Default::default()
        };
        assert!(!runner_matches_requirements(&runner, &req));
    }

    #[test]
    fn cpu_meets_minimum() {
        let runner = make_runner();
        let req = RunnerRequirements {
            min_cpu: Some(4),
            ..Default::default()
        };
        assert!(runner_matches_requirements(&runner, &req));
    }

    #[test]
    fn cpu_below_minimum() {
        let runner = make_runner();
        let req = RunnerRequirements {
            min_cpu: Some(16),
            ..Default::default()
        };
        assert!(!runner_matches_requirements(&runner, &req));
    }

    #[test]
    fn arch_mismatch() {
        let runner = make_runner();
        let req = RunnerRequirements {
            arch: Some("aarch64".to_string()),
            ..Default::default()
        };
        assert!(!runner_matches_requirements(&runner, &req));
    }

    #[test]
    fn combined_requirements() {
        let runner = make_runner();
        let req = RunnerRequirements {
            runner_name: Some("fast".to_string()),
            required_tags: vec!["linux".to_string()],
            min_cpu: Some(4),
            min_memory_mb: Some(8192),
            min_gpu: Some(1),
            arch: Some("x86_64".to_string()),
        };
        assert!(runner_matches_requirements(&runner, &req));
    }
}
