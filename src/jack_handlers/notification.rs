use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

pub struct Handler {
    sample_rate: Arc<AtomicUsize>,
}

impl Handler {
    pub fn new(sample_rate: Arc<AtomicUsize>) -> Self {
        Handler { sample_rate }
    }
}

impl jack::NotificationHandler for Handler {
    /// Called whenever the system sample rate changes.
    fn sample_rate(&mut self, _client: &jack::Client, srate: jack::Frames) -> jack::Control {
        self.sample_rate.store(srate as usize, Ordering::SeqCst);
        jack::Control::Continue
    }
}
