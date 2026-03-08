//! Sampler module - Pad and Sample management

pub mod bank;
pub mod library;
pub mod pad;
pub mod sample;
pub mod thumbnail;

pub use bank::{BankManager, SampleBank};
pub use pad::{SamplePad, TriggerMode};
pub use sample::VideoSample;
