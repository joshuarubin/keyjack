use anyhow::Result;
use signal_hook::{
    consts::TERM_SIGNALS,
    iterator::{exfiltrator::SignalOnly, SignalsInfo},
};
use std::{
    f64::consts::PI,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};
use structopt::StructOpt;

#[derive(StructOpt)]
struct Opt {
    #[structopt(short = "j", default_value = "keyjack")]
    jack_client_name: String,

    #[structopt(short = "f", default_value = "550")]
    sidetone_freq: f64,
}

fn handle_signals() -> Result<()> {
    // register SIGTERM, SIGQUIT, SIGINT signals
    let term = Arc::new(AtomicBool::new(false));
    for sig in TERM_SIGNALS {
        signal_hook::flag::register(*sig, Arc::clone(&term))?;
    }

    let mut signals = SignalsInfo::<SignalOnly>::new(TERM_SIGNALS)?;

    // listen for SIGTERM, SIGQUIT, SIGINT
    for sig in &mut signals {
        println!("signal {:?}", sig);
        break;
    }

    Ok(())
}

fn main() -> Result<()> {
    let opt = Opt::from_args();

    // create jack client
    let (client, _status) = jack::Client::new(
        &opt.jack_client_name[..],
        jack::ClientOptions::NO_START_SERVER,
    )?;

    // register the output port
    let port = client.register_port("out", jack::AudioOut)?;

    let sample_rate = Arc::new(AtomicUsize::new(client.sample_rate()));

    // create the async client
    let _aclient = client.activate_async(
        NotificationHandler::new(Arc::clone(&sample_rate)),
        ProcessHandler::new(port, opt.sidetone_freq, Arc::clone(&sample_rate)),
    )?;

    handle_signals()
}

struct NotificationHandler {
    sample_rate: Arc<AtomicUsize>,
}

impl NotificationHandler {
    fn new(sample_rate: Arc<AtomicUsize>) -> Self {
        NotificationHandler { sample_rate }
    }
}

impl jack::NotificationHandler for NotificationHandler {
    /// Called whenever the system sample rate changes.
    fn sample_rate(&mut self, _client: &jack::Client, srate: jack::Frames) -> jack::Control {
        self.sample_rate.store(srate as usize, Ordering::SeqCst);
        jack::Control::Continue
    }
}

struct ProcessHandler {
    port: jack::Port<jack::AudioOut>,
    sidetone_freq: f64,
    sample_rate: Arc<AtomicUsize>,
}

impl ProcessHandler {
    fn new(
        port: jack::Port<jack::AudioOut>,
        sidetone_freq: f64,
        sample_rate: Arc<AtomicUsize>,
    ) -> Self {
        ProcessHandler {
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

impl jack::ProcessHandler for ProcessHandler {
    fn process(
        &mut self,
        _client: &jack::Client,
        process_scope: &jack::ProcessScope,
    ) -> jack::Control {
        self.write_sine(process_scope);

        jack::Control::Continue
    }
