use std::error::Error;
use std::fmt::Formatter;

use rppal::gpio::{Gpio, IoPin, Mode};

/// Temperature, in degrees celsius
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(transparent)]
pub struct TemperatureCelsius(f64);

impl From<TemperatureCelsius> for f64 {
    fn from(v: TemperatureCelsius) -> Self {
        v.0
    }
}

impl From<f64> for TemperatureCelsius {
    fn from(v: f64) -> Self {
        Self(v)
    }
}

impl std::fmt::Display for TemperatureCelsius {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}c", self.0)
    }
}

/// Relative humidity (from 0 to 100)
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(transparent)]
pub struct Humidity(f64);

impl From<Humidity> for f64 {
    fn from(v: Humidity) -> Self {
        v.0
    }
}

impl From<f64> for Humidity {
    fn from(v: f64) -> Self {
        Self(v)
    }
}

impl std::fmt::Display for Humidity {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}%", self.0)
    }
}

/// Potential kinds of errors that can be encountered reading from the DHT sensor
#[derive(PartialEq, Eq, Debug, Hash, Clone, Copy)]
pub enum SensorErrorKind {
    Initialization,
    ReadTimeout,
}

/// Error initializing or reading the DHT22 sensor via a GPIO pin
#[derive(Debug)]
pub enum SensorError {
    ValueBounds(String),
    CheckSum(u8, u8),
    KindMsg(SensorErrorKind, &'static str),
    KindMsgCause(SensorErrorKind, &'static str, Box<dyn Error + Send + Sync>),
}

impl std::fmt::Display for SensorError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SensorError::ValueBounds(msg) => {
                write!(f, "{}", msg)
            }
            SensorError::CheckSum(expected, got) => {
                write!(f, "checksum error: expected {}, got {}", expected, got)
            }
            SensorError::KindMsg(_, msg) => msg.fmt(f),
            SensorError::KindMsgCause(_, msg, ref e) => write!(f, "{}: {}", msg, e),
        }
    }
}

impl Error for SensorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SensorError::KindMsgCause(_, _, ref e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

/// Create a new `IoPin` based on the BCM GPIO pin number of the data wire of a
/// sensor.
///
/// Note that the BCM GPIO pin number is NOT the same as the physical pin number.
/// See [pinout] for more information.
///
/// [pinout]: https://www.raspberrypi.com/documentation/computers/os.html#gpio-and-the-40-pin-header
pub fn open_pin(bcm_gpio_pin: u8, mode: Mode) -> Result<IoPin, SensorError> {
    let controller = Gpio::new().map_err(|e| {
        SensorError::KindMsgCause(
            SensorErrorKind::Initialization,
            "unable to create GPIO controller",
            Box::new(e),
        )
    })?;

    let pin = controller.get(bcm_gpio_pin).map_err(|e| {
        SensorError::KindMsgCause(
            SensorErrorKind::Initialization,
            "unable to acquire pin from controller",
            Box::new(e),
        )
    })?;

    let io_pin = pin.into_io(mode);
    Ok(io_pin)
}

/// Abstraction around an `rppal::gpio::IoPin` to allow for easier testing.
pub trait DataPin {
    fn is_low(&self) -> bool;
    fn is_high(&self) -> bool;
    fn pin(&self) -> u8;
    fn set_high(&mut self);
    fn set_low(&mut self);
    fn set_mode(&mut self, mode: Mode);
}

impl DataPin for IoPin {
    fn is_low(&self) -> bool {
        IoPin::is_low(self)
    }

    fn is_high(&self) -> bool {
        IoPin::is_high(self)
    }

    fn pin(&self) -> u8 {
        IoPin::pin(self)
    }

    fn set_high(&mut self) {
        IoPin::set_high(self);
    }

    fn set_low(&mut self) {
        IoPin::set_low(self);
    }

    fn set_mode(&mut self, mode: Mode) {
        IoPin::set_mode(self, mode);
    }
}
