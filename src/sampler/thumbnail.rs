//! Thumbnail generation and caching

use std::collections::HashMap;
use uuid::Uuid;

pub struct ThumbnailCache {
    cache: HashMap<Uuid, wgpu::Texture>,
}

impl ThumbnailCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }
    
    pub fn get(&self, id: Uuid) -> Option<&wgpu::Texture> {
        self.cache.get(&id)
    }
    
    pub fn insert(&mut self, id: Uuid, texture: wgpu::Texture) {
        self.cache.insert(id, texture);
    }
}
