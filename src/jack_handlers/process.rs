use std::{
    f64::consts::PI,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

pub struct Handler {
    port: jack::Port<jack::AudioOut>,
    sidetone_freq: f64,
    sample_rate: Arc<AtomicUsize>,
}

impl Handler {
    pub fn new(
        port: jack::Port<jack::AudioOut>,
        sidetone_freq: f64,
        sample_rate: Arc<AtomicUsize>,
    ) -> Self {
        Handler {
            port,
            sidetone_freq,
            sample_rate,
        }
    }

    fn write_sine(&mut self, process_scope: &jack::ProcessScope) {
        let sample_rate = self.sample_rate.load(Ordering::SeqCst);
        let two_pi_freq_per_rate = (2. * PI * self.sidetone_freq) / sample_rate as f64;
        let buf = self.port.as_mut_slice(process_scope);

        for (n, val) in buf.iter_mut().enumerate() {
            let pos = (process_scope.last_frame_time() as usize + n) as f64;
            *val = (two_pi_freq_per_rate * pos).sin() as f32;
        }
    }
}

impl jack::ProcessHandler for Handler {
    /// Called whenever there is work to be done.
    ///
    /// It needs to be suitable for real-time execution. That means that it
    /// cannot call functions
    /// that might block for a long time. This includes all I/O functions
    /// (disk, TTY, network),
    /// malloc, free, printf, pthread_mutex_lock, sleep, wait, poll, select,
    /// pthread_join,
    /// pthread_cond_wait, etc, etc.
    ///
    /// Should return `Control::Continue` on success, and
    /// `Control::Quit` on error.
    fn process(
        &mut self,
        _client: &jack::Client,
        process_scope: &jack::ProcessScope,
    ) -> jack::Control {
        self.write_sine(process_scope);

        jack::Control::Continue
    }
}
