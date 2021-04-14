mod jack_handlers;
mod winkey;

use anyhow::Result;
use jack_handlers::{notification, process};
use mio::{Events, Interest, Poll, Token};
use signal_hook::consts::TERM_SIGNALS;
use signal_hook_mio::v0_7::Signals;
use std::{
    io::ErrorKind,
    sync::{atomic::AtomicUsize, Arc},
};
use structopt::StructOpt;

#[cfg(unix)]
const DEFAULT_TTY: &str = "/dev/ttyUSB0";
#[cfg(windows)]
const DEFAULT_TTY: &str = "COM1";

const SIGNAL: Token = Token(0);
const SERIAL: Token = Token(1);

#[derive(StructOpt)]
struct Opt {
    #[structopt(short = "j", default_value = "keyjack")]
    jack_client_name: String,

    #[structopt(short = "f", default_value = "550")]
    sidetone_freq: f64,

    #[structopt(short = "p", default_value = DEFAULT_TTY)]
    serial_port: String,
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
        notification::Handler::new(Arc::clone(&sample_rate)),
        process::Handler::new(port, opt.sidetone_freq, Arc::clone(&sample_rate)),
    )?;

    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(16);

    // register SIGTERM, SIGQUIT, SIGINT signals
    let mut signals = Signals::new(TERM_SIGNALS)?;
    poll.registry()
        .register(&mut signals, SIGNAL, Interest::READABLE)?;

    // initialize the keyer
    let mut keyer = winkey::Client::new(opt.serial_port, poll.registry(), SERIAL)?;

    // main event loop
    loop {
        poll.poll(&mut events, None).or_else(|e| {
            if e.kind() == ErrorKind::Interrupted {
                events.clear();
                Ok(())
            } else {
                Err(e)
            }
        })?;

        for event in events.iter() {
            match event.token() {
                SIGNAL => return Ok(()),
                SERIAL => keyer.read()?,
                _ => unreachable!(),
            }
        }
    }
}
