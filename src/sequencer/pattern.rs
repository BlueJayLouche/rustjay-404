use super::track::SequencerTrack;
use serde::{Deserialize, Serialize};

/// A pattern contains 16 tracks (one per pad)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub index: usize,
    pub name: String,
    pub tracks: [SequencerTrack; 16],
}

impl Pattern {
    pub fn new(index: usize) -> Self {
        let tracks = std::array::from_fn(|i| SequencerTrack::new(i));
        
        Self {
            index,
            name: format!("Pattern {:02}", index + 1),
            tracks,
        }
    }

    /// Get a track by pad index
    pub fn get_track(&self, pad_index: usize) -> Option<&SequencerTrack> {
        self.tracks.get(pad_index)
    }

    /// Get mutable track
    pub fn get_track_mut(&mut self, pad_index: usize) -> Option<&mut SequencerTrack> {
        self.tracks.get_mut(pad_index)
    }

    /// Set all tracks to the same length
    pub fn set_length(&mut self, length: usize) {
        for track in &mut self.tracks {
            track.set_length(length);
        }
    }

    /// Get the common length (returns first track's length)
    pub fn length(&self) -> usize {
        self.tracks.first().map(|t| t.length).unwrap_or(16)
    }

    /// Clear all tracks
    pub fn clear(&mut self) {
        for track in &mut self.tracks {
            track.clear();
        }
    }

    /// Randomize all tracks
    pub fn randomize(&mut self, density: f32) {
        for track in &mut self.tracks {
            track.randomize(density);
        }
    }

    /// Copy from another pattern
    pub fn copy_from(&mut self, other: &Pattern) {
        self.tracks = other.tracks.clone();
    }

    /// Get active pads in this pattern
    pub fn active_pads(&self) -> Vec<usize> {
        self.tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.steps.iter().any(|s| s.active))
            .map(|(i, _)| i)
            .collect()
    }
}

impl Default for Pattern {
    fn default() -> Self {
        Self::new(0)
    }
}
