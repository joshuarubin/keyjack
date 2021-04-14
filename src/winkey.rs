use anyhow::{anyhow, Result};
use mio::Interest;
use mio_serial::SerialPortBuilderExt;
use serialport::SerialPortBuilder;
use std::{io, io::ErrorKind, io::Write, ops, time::Duration};

const LOW_BAUD: u32 = 1200;
const HIGH_BAUD: u32 = 9600;

const MIN_SPEED: u8 = 13;
const DEFAULT_SPEED: u8 = 30;
const MAX_SPEED: u8 = 45;

struct Command(&'static [u8]);

impl Command {
    const ADMIN_HOST_OPEN: Command = Command(&[0, 2]);
    const ADMIN_HOST_CLOSE: Command = Command(&[0, 3]);
    const ADMIN_SET_WK2_MODE: Command = Command(&[0, 11]);
    const ADMIN_SET_HIGH_BAUD: Command = Command(&[0, 17]);

    const SET_WK2_MODE: Command = Command(&[0xe]);
    const SET_WPM_SPEED: Command = Command(&[0x2]);
    const SETUP_SPEED_POT: Command = Command(&[5]);

    fn send<W>(self, w: &mut W, extra: Option<Vec<u8>>) -> io::Result<usize>
    where
        W: io::Write,
    {
        let mut data = self.0.to_vec();
        if extra.is_some() {
            data = [data, extra.unwrap().to_vec()].concat();
        }
        w.write(&data)
    }
}

const WINKEY_23: u8 = 23;
const STATUS_BYTE: u8 = 0xc0;
const SPEED_POT_BYTE: u8 = 0x80;
const SPEED_MASK: u8 = !(1 << 7);

struct Mode(u8);

impl Mode {
    const PADDLE_ECHO_BACK: Mode = Mode(1 << 6);
    const KEY_MODE_IAMBIC_B: Mode = Mode(0);
    const SERIAL_ECHOBACK: Mode = Mode(1 << 2);

    fn option(&self) -> Option<Vec<u8>> {
        return Some([self.0].to_vec());
    }
}

impl ops::BitOr for Mode {
    type Output = Self;

    #[inline]
    fn bitor(self, other: Self) -> Self {
        Mode(self.0 | other.0)
    }
}

pub struct Client {
    serial: Box<dyn mio_serial::MioSerialPort>,
    buf: Vec<u8>,
    status: u8,
}

impl Client {
    pub fn new(path: String, registry: &mio::Registry, token: mio::Token) -> Result<Self> {
        let slow_builder = mio_serial::new(path, LOW_BAUD)
            .data_bits(mio_serial::DataBits::Eight)
            .stop_bits(mio_serial::StopBits::Two)
            .parity(mio_serial::Parity::None);

        let fast_builder = slow_builder.clone().baud_rate(HIGH_BAUD);

        Client::slow_init(slow_builder)?;

        let mut serial = fast_builder.open_async()?;

        registry.register(&mut serial, token, Interest::READABLE)?;

        let mut client = Client {
            serial,
            buf: vec![0u8; 1024],
            status: 0,
        };

        client.initialize()?;

        Ok(client)
    }

    fn slow_init(builder: SerialPortBuilder) -> Result<()> {
        let mut serial = builder.timeout(Duration::from_millis(1000)).open()?;

        Command::ADMIN_SET_WK2_MODE.send(&mut serial, None)?;
        Command::ADMIN_HOST_OPEN.send(&mut serial, None)?;

        let mut buf = [0; 1];
        serial.read_exact(&mut buf)?;

        if buf[0] != WINKEY_23 {
            return Err(anyhow!(
                "only winkey 2.3 is supported, received version {:?}",
                buf[0]
            ));
        }

        Command::ADMIN_SET_HIGH_BAUD.send(&mut serial, None)?;
        std::thread::sleep(Duration::from_millis(400));

        Ok(())
    }

    fn initialize(&mut self) -> Result<()> {
        let mode = Mode::PADDLE_ECHO_BACK | Mode::KEY_MODE_IAMBIC_B | Mode::SERIAL_ECHOBACK;

        Command::SET_WK2_MODE.send(self, mode.option())?;

        println!("WPM: {}", DEFAULT_SPEED);
        Command::SET_WPM_SPEED.send(self, Some(vec![DEFAULT_SPEED]))?;

        Command::SETUP_SPEED_POT.send(self, Some(vec![MIN_SPEED, MAX_SPEED - MIN_SPEED, 0]))?;

        Ok(())
    }

    pub fn read(&mut self) -> Result<()> {
        loop {
            match self.serial.read(&mut self.buf[..]) {
                Ok(count) => self.on_receive(count)?,
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    break;
                }
                Err(e) => {
                    println!("Quitting due to read error: {}", e);
                    return Err(anyhow!(e));
                }
            }
        }
        Ok(())
    }

    fn on_receive(&mut self, count: usize) -> Result<()> {
        let data = &self.buf[..count];
        match data[0] & STATUS_BYTE {
            STATUS_BYTE => self.status = data[0],
            SPEED_POT_BYTE => {
                println!("\nWPM: {}", (data[0] & SPEED_MASK) + MIN_SPEED);
            }
            _ => {
                print!("{}", data[0] as char);
                io::stdout().flush()?;
            }
        }

        Ok(())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        Command::ADMIN_HOST_CLOSE
            .send(self, None)
            .unwrap_or_else(|e| -> usize {
                println!("\nerror closing host {}", e);
                0
            });
    }
}

impl io::Write for Client {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.serial.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.serial.flush()
    }
}
