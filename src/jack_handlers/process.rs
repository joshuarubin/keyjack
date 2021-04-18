use anyhow::Result;
use std::{
    f64::consts::PI,
    slice,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

pub struct Handler {
    port: jack::Port<TerminalAudioOut>,
    sidetone_freq: f64,
    sample_rate: Arc<AtomicUsize>,
    tx_key_line: Arc<AtomicBool>,
    last_tx_key_line: bool,
    start_frame_time: u32,
    start_finishing_frame_time: u32,
}

struct TerminalAudioOut;

const VOLUME_SCALE: f32 = 0.6;

unsafe impl<'a> jack::PortSpec for TerminalAudioOut {
    fn jack_port_type(&self) -> &'static str {
        jack_sys::FLOAT_MONO_AUDIO
    }

    fn jack_flags(&self) -> jack::PortFlags {
        jack::PortFlags::IS_OUTPUT | jack::PortFlags::IS_TERMINAL
    }

    fn jack_buffer_size(&self) -> libc::c_ulong {
        // Not needed for built in types according to JACK api
        0
    }
}

fn output_mut_buf<'a>(
    port: &'a mut jack::Port<TerminalAudioOut>,
    ps: &'a jack::ProcessScope,
) -> &'a mut [f32] {
    assert_eq!(port.client_ptr(), ps.client_ptr());
    unsafe {
        slice::from_raw_parts_mut(
            port.buffer(ps.n_frames()) as *mut f32,
            ps.n_frames() as usize,
        )
    }
}

const RISE_TIME: Duration = Duration::from_millis(5);

impl Handler {
    pub fn new(
        client: &jack::Client,
        sidetone_freq: f64,
        sample_rate: Arc<AtomicUsize>,
        tx_key_line: Arc<AtomicBool>,
    ) -> Result<Self> {
        // register the output port
        let port = client.register_port("out", TerminalAudioOut)?;

        Ok(Handler {
            port,
            sidetone_freq,
            sample_rate,
            tx_key_line,
            last_tx_key_line: false,
            start_frame_time: 0,
            start_finishing_frame_time: 0,
        })
    }

    fn write_sine(&mut self, process_scope: &jack::ProcessScope) {
        let sample_rate = self.sample_rate.load(Ordering::SeqCst);
        let two_pi_freq_per_rate = (2. * PI * self.sidetone_freq) / sample_rate as f64;
        let buf = output_mut_buf(&mut self.port, process_scope);
        let sample_dur = Duration::from_secs_f64(1. / sample_rate as f64);
        let rise_samples = (RISE_TIME.as_secs_f64() / sample_dur.as_secs_f64()).round();

        let pos = (process_scope.last_frame_time() - self.start_frame_time) as usize;
        let finish_pos =
            (process_scope.last_frame_time() - self.start_finishing_frame_time) as usize;

        let mut finished = false;
        for (n, val) in buf.iter_mut().enumerate() {
            if finished {
                *val = 0.;
                continue;
            }

            *val = (two_pi_freq_per_rate * (pos + n) as f64).sin() as f32;

            // rise time
            if pos + n <= rise_samples as usize {
                *val *= ((pos + n) as f64 / rise_samples) as f32;
            }

            // fall time
            if self.start_finishing_frame_time > 0 {
                if finish_pos + n <= rise_samples as usize {
                    *val *= 1. - ((finish_pos + n) as f64 / rise_samples) as f32;
                } else {
                    finished = true;
                    self.start_finishing_frame_time = 0;
                    *val = 0.;
                }
            }

            *val *= VOLUME_SCALE;
        }
    }

    fn clear_buf(&mut self, process_scope: &jack::ProcessScope) {
        let buf = output_mut_buf(&mut self.port, process_scope);
        for (_n, val) in buf.iter_mut().enumerate() {
            *val = 0.;
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
        let tx_key_line = self.tx_key_line.load(Ordering::SeqCst);

        if tx_key_line != self.last_tx_key_line {
            if tx_key_line {
                if self.start_finishing_frame_time > 0 {
                    self.start_finishing_frame_time = 0;
                } else if self.start_frame_time == 0 {
                    self.start_frame_time = process_scope.last_frame_time();
                }
            } else {
                self.start_finishing_frame_time = process_scope.last_frame_time();
            }
        }
        self.last_tx_key_line = tx_key_line;

        if tx_key_line || self.start_finishing_frame_time > 0 {
            self.write_sine(process_scope);
        } else {
            self.start_frame_time = 0;
            self.clear_buf(process_scope);
        }

        jack::Control::Continue
    }
}
