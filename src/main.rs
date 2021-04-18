mod jack_handlers;
mod winkey;

use anyhow::Result;
use jack_handlers::{notification, process};
use mio::{Events, Interest, Poll, Token};
use signal_hook::consts::TERM_SIGNALS;
use signal_hook_mio::v0_7::Signals;
use std::{
    io::ErrorKind,
    sync::{
        atomic::{AtomicBool, AtomicUsize},
        Arc,
    },
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

    let mut poll = Poll::new()?;

    let tx_key_line = Arc::new(AtomicBool::new(false));

    // initialize the keyer
    let mut keyer = winkey::Client::new(
        opt.serial_port,
        poll.registry(),
        SERIAL,
        Arc::clone(&tx_key_line),
    )?;

    // create jack client
    let (client, _status) = jack::Client::new(
        &opt.jack_client_name[..],
        jack::ClientOptions::NO_START_SERVER,
    )?;

    let sample_rate = Arc::new(AtomicUsize::new(client.sample_rate()));

    let ph = process::Handler::new(
        &client,
        opt.sidetone_freq,
        Arc::clone(&sample_rate),
        Arc::clone(&tx_key_line),
    )?;

    // create the async client
    let _aclient =
        client.activate_async(notification::Handler::new(Arc::clone(&sample_rate)), ph)?;

    // register SIGTERM, SIGQUIT, SIGINT signals
    let mut signals = Signals::new(TERM_SIGNALS)?;
    poll.registry()
        .register(&mut signals, SIGNAL, Interest::READABLE)?;

    // main event loop
    let mut events = Events::with_capacity(16);
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
