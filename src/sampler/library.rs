//! Sample library management

use std::path::PathBuf;

pub struct SampleLibrary {
    root_path: PathBuf,
}

impl SampleLibrary {
    pub fn new(root: PathBuf) -> Self {
        Self { root_path: root }
    }
    
    pub fn scan(&self) -> Vec<PathBuf> {
        // TODO: Scan for video files
        vec![]
    }
}
