use anyhow::Result;
use biquad::{coefficients::Coefficients, Biquad, ToHertz, Type};
use std::{
    f64::consts::PI,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

pub struct Handler {
    port: jack::Port<jack::AudioOut>,
    sidetone_freq: f64,
    sample_rate: Arc<AtomicUsize>,
    last_sample_rate: usize,
    tx_key_line: Arc<AtomicBool>,
    last_tx_key_line: bool,
    tx_start_frame_time: u32,
    filter: biquad::DirectForm2Transposed<f64>,
    volume: f32,
}

const GAIN: f32 = 0.5;

fn coefficients(sample_rate: usize, sidetone_freq: f64) -> Coefficients<f64> {
    Coefficients::<f64>::from_params(
        Type::LowPass,
        (sample_rate as f64).hz(),
        (sidetone_freq * 1.1).hz(),
        1.,
    )
    .unwrap()
}

impl Handler {
    pub fn new(
        client: &jack::Client,
        sidetone_freq: f64,
        sample_rate: Arc<AtomicUsize>,
        tx_key_line: Arc<AtomicBool>,
        volume: f32,
    ) -> Result<Self> {
        let sr = sample_rate.load(Ordering::SeqCst);

        Ok(Handler {
            // register the output port
            port: client.register_port("out", jack::AudioOut)?,
            sidetone_freq,
            sample_rate,
            last_sample_rate: sr,
            tx_key_line,
            last_tx_key_line: false,
            tx_start_frame_time: 0,
            filter: biquad::DirectForm2Transposed::<f64>::new(coefficients(sr, sidetone_freq)),
            volume,
        })
    }

    fn write_buf(&mut self, process_scope: &jack::ProcessScope) {
        let step = (2. * PI * self.sidetone_freq) / self.last_sample_rate as f64;
        let buf = self.port.as_mut_slice(process_scope);
        let pos = (process_scope.last_frame_time() - self.tx_start_frame_time) as usize;

        for (n, val) in buf.iter_mut().enumerate() {
            if self.last_tx_key_line {
                *val = self.filter.run((step * (pos + n) as f64).sin()) as f32;
            } else {
                *val = self.filter.run(0.) as f32;
            }

            *val *= GAIN * self.volume;
        }
    }

    fn update_sample_rate(&mut self) {
        let sample_rate = self.sample_rate.load(Ordering::SeqCst);
        if sample_rate == self.last_sample_rate {
            return;
        }

        self.filter
            .update_coefficients(coefficients(sample_rate, self.sidetone_freq));
        self.last_sample_rate = sample_rate;
    }

    fn update_tx_key_line(&mut self, process_scope: &jack::ProcessScope) {
        let tx_key_line = self.tx_key_line.load(Ordering::SeqCst);

        if tx_key_line == self.last_tx_key_line {
            return;
        }

        if tx_key_line {
            self.tx_start_frame_time = process_scope.last_frame_time();
        }

        self.last_tx_key_line = tx_key_line;
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
        self.update_sample_rate();
        self.update_tx_key_line(process_scope);
        self.write_buf(process_scope);

        jack::Control::Continue
    }
}
