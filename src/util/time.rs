//! Time utilities

use std::time::{Duration, Instant};

/// FPS counter
pub struct FpsCounter {
    last_time: Instant,
    frame_count: u32,
    fps: f32,
}

impl FpsCounter {
    pub fn new() -> Self {
        Self {
            last_time: Instant::now(),
            frame_count: 0,
            fps: 0.0,
        }
    }
    
    pub fn update(&mut self) -> f32 {
        self.frame_count += 1;
        let elapsed = self.last_time.elapsed();
        
        if elapsed >= Duration::from_secs(1) {
            self.fps = self.frame_count as f32 / elapsed.as_secs_f32();
            self.frame_count = 0;
            self.last_time = Instant::now();
        }
        
        self.fps
    }
}
