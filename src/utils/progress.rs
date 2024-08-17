use std::hash::{Hash, Hasher};

#[derive(Debug, Clone)]
pub struct Progress {
    job_name: String,
    progress: usize,
    total_size: usize,
}

impl Progress {
    pub fn finished(&self) -> bool {
        self.progress == self.total_size
    }
    pub fn progress(&self) -> usize {
        self.progress
    }

    pub fn total_size(&self) -> usize {
        self.total_size
    }

    pub fn job_name(&self) -> &String {
        &self.job_name
    }
}

impl Hash for Progress {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.job_name.hash(state);
    }
}

impl PartialEq for Progress {
    fn eq(&self, other: &Self) -> bool {
        self.job_name == other.job_name
    }
}

impl Eq for Progress {}
