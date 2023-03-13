use std::fmt::{Debug, Formatter};
use std::thread;
use std::time::Duration;

use log::debug;
use rppal::gpio::Mode;

use crate::sensor::{DataPin, Humidity, SensorError, SensorErrorKind, TemperatureCelsius};

pub(crate) const DHT_MAX_COUNT: u32 = 32_000;
// 800;
pub(crate) const DHT_PULSES: usize = 41;
pub(crate) const DATA_SIZE: usize = 5;

/// Cycle counts of how long the sensor data pin spent low and high states.
///
/// There are 40 low/high transitions we count cycles for. These counts are
/// used to read 40 bits of information from the sensor.
#[derive(Debug)]
struct Pulses {
    // We store counts for 41 transitions but don't use the first low/high transition
    counts: [u32; DHT_PULSES * 2],
}

impl Pulses {
    /// Count the number of cycles the given pin spends in the low and high states for
    /// 40 low/high transitions.
    ///
    /// An error will be returned if the pin didn't transition in time. The read will have
    /// to be retried in this case.
    ///
    /// NOTE: This method assumes the pin as already been prepared for reading by sending
    /// and initial high-low-high transition with timings corresponding to the DHT22
    /// datasheet.
    fn from_data_pin(pin: &dyn DataPin) -> Result<Self, SensorError> {
        // Create an array with 2x the number of pulses we're going to measure so that we can
        // store the number of cycles the pin spent high and low for each pulse.
        let mut counts: [u32; DHT_PULSES * 2] = [0; DHT_PULSES * 2];

        // Store counts for both high and low states of the pin in the same array. We advance
        // by two entries each iteration of the loop but use (i + 1) to access the odd entries.
        //
        // We only store up to DHT_MAX_COUNT which is a much much higher number of cycles than
        // we expect to get in practice (normal number of cycles at high or low is 50 - 200).
        // This is done to enforce a timeout while waiting for the pin to switch between low
        // and high states. In this case, the read will have to be retried.
        for i in (0..counts.len()).step_by(2) {
            while pin.is_low() {
                counts[i] += 1;
                if counts[i] >= DHT_MAX_COUNT as u32 {
                    return Err(SensorError::KindMsg(
                        SensorErrorKind::ReadTimeout,
                        "timeout waiting for low pulse capture",
                    ));
                }
            }

            while pin.is_high() {
                counts[i + 1] += 1;
                if counts[i + 1] >= DHT_MAX_COUNT as u32 {
                    return Err(SensorError::KindMsg(
                        SensorErrorKind::ReadTimeout,
                        "timeout waiting for high pulse capture",
                    ));
                }
            }
        }

        // tracing::trace!(message = "reading low/high pulse counts", counts = ?counts);
        Ok(Self { counts })
    }

    /// Return an iterator over 40 cycle counts for the pin in the low state.
    fn low(&self) -> impl ExactSizeIterator<Item = &u32> {
        // Start from the 3rd element (first valid low count), emitting only low counts.
        // We're skipping the first low/high transition since the pin starts in the low
        // state when reading data and thus the first cycle count is always zero.
        self.counts.iter().skip(2).step_by(2)
    }

    /// Return an iterator over 40 cycle counts for the pin in the high state.
    fn high(&self) -> impl ExactSizeIterator<Item = &u32> {
        // Start from the 4th element (first valid high count), emitting only high counts.
        // We're skipping the first low/high transition since the pin starts in the low
        // state when reading data and thus the first cycle count is always zero.
        self.counts.iter().skip(3).step_by(2)
    }
}

/// Bytes read from a sensor, computed from high/low pulse cycle counts.
///
/// Bytes read make up temperature data, humidity data, and a checksum to ensure
/// the reading is valid. If valid, the reading can be converted to a temperature
/// and humidity valid.
#[derive(Debug)]
struct Reading {
    bytes: [u8; DATA_SIZE],
}

impl Reading {
    fn from_pulses(pulses: &Pulses) -> Result<Self, SensorError> {
        let mut bytes: [u8; DATA_SIZE] = [0; DATA_SIZE];
        let mut bits: [u8; 40] = [0; 40];

        // Find the average low pin cycle count so that we can determine if each high
        // pin cycle count is meant to be a 0 bit (lower than the threshold) or a 1 bit
        // (higher than the threshold).
        let threshold = pulses.low().sum::<u32>() / pulses.low().len() as u32;
        debug!("threshold: {}", threshold);

        for (i, &v) in pulses.high().enumerate() {
            // There are 40 low/high transition cycle counts and hence 40 bits of data
            // that we need to parse. Divide by eight to figure out which byte this bit
            // will end up in and shift the current value left (we only operate on the
            // LSB each iteration).
            let index = i / 8;
            bytes[index] <<= 1;

            if v >= threshold {
                bits[i] = 1;
                bytes[index] |= 1;
            }
        }

        // Byte five is a checksum of the first four bytes, return an error if it indicates
        // the data we've read is corrupt somehow.
        debug!("bits    : {:?}", bits);
        debug!("humidity: {:?}", &bits[0..16]);
        debug!("temp    : {:?}", &bits[16..32]);
        debug!("chksum  : {:?}", &bits[32..40]);
        debug!("bytes  : {:?}", &bytes);
        Self::checksum_bytes(&bytes)?;
        Ok(Reading { bytes })
    }

    fn checksum_bytes(bytes: &[u8; DATA_SIZE]) -> Result<(), SensorError> {
        // From the DHT22 datasheet:
        // > If the data transmission is right, check-sum should be the last 8 bit of
        // > "8 bit integral RH data+8 bit decimal RH data+8 bit integral T data+8 bit
        // > decimal T data".
        let expected = bytes[4];
        let computed =
            ((bytes[0] as u16 + bytes[1] as u16 + bytes[2] as u16 + bytes[3] as u16) & 0xFF) as u8;

        if computed != expected {
            Err(SensorError::CheckSum(expected, computed))
        } else {
            Ok(())
        }
    }
}

impl From<Reading> for (TemperatureCelsius, Humidity) {
    /// Convert a `Reading` sensor reading into temperature and humidity measurements.
    ///
    /// This conversion is guaranteed to succeed because the checksum enforced during creation
    /// of instances of `Reading` ensures the bytes read from the sensor are valid.
    fn from(reading: Reading) -> Self {
        // See https://cdn-shop.adafruit.com/datasheets/Digital+humidity+and+temperature+sensor+AM2302.pdf
        // first two bytes are humidity as a u16 * 10
        let humidity_raw =
            (reading.bytes[0] as u16) * 256 /* shift left 8 bits */ + reading.bytes[1] as u16;
        // second two bytes are temperature as a u16 * 10 with the highest bit indicating sign
        let temp_raw = ((reading.bytes[2] & 0b0111_1111) as u16) * 256 /* shift left 8 bits */ + reading.bytes[3] as u16;

        let humidity_dec = humidity_raw as f64 / 10.0;
        let mut temp_dec = temp_raw as f64 / 10.0;
        // highest bit of the temperature is `1` to indicate a negative value
        if reading.bytes[2] & 0b1000_0000 > 0 {
            temp_dec = -temp_dec;
        }

        let humidity = Humidity::from(humidity_dec);
        let temperature = TemperatureCelsius::from(temp_dec);

        (temperature, humidity)
    }
}

/// Read temperature in degrees celsius and relative humidity from a DHT22 sensor
pub struct DHT22Sensor {
    pin: Box<dyn DataPin + Send + Sync + 'static>,
}

impl DHT22Sensor {
    pub fn from_pin<T>(pin: T) -> Self
    where
        T: DataPin + Send + Sync + 'static,
    {
        Self { pin: Box::new(pin) }
    }

    fn prepare_for_read(&mut self) {
        // https://cdn-shop.adafruit.com/datasheets/Digital+humidity+and+temperature+sensor+AM2302.pdf
        // Host needs to set the sensor:
        // * high to start the read process, waking the sensor up from low-power mode
        // * low for at least 1ms to ensure the sensor detected the start of this process
        // * high for 20-40us to then wait for the sensor's response
        self.pin.set_mode(Mode::Output);
        self.pin.set_high();
        thread::sleep(Duration::from_millis(10));
        self.pin.set_low();
        thread::sleep(Duration::from_millis(20));
        self.pin.set_high();
        thread::sleep(Duration::from_micros(30));
        self.pin.set_mode(Mode::Input);
    }

    /// Read temperature and humidity from the sensor or return an error if the
    /// read failed with details about what caused the read to fail.
    pub fn read(&mut self) -> Result<(TemperatureCelsius, Humidity), SensorError> {
        self.prepare_for_read();
        let pulses = Pulses::from_data_pin(self.pin.as_ref())?;
        debug!("pulses: {:?}", &pulses.counts[..]);
        let data = Reading::from_pulses(&pulses)?;
        let (temp, hum): (TemperatureCelsius, Humidity) = data.into();
        let temp_c = f64::from(temp);
        if temp_c > 80.0 || temp_c < -80.0 {
            Err(SensorError::ValueBounds(format!(
                "temp_c {} is out of bounds",
                temp_c
            )))
        } else {
            Ok((temp, hum))
        }
    }
}

impl Debug for DHT22Sensor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DHT22Sensor")
            .field("pin", &self.pin.pin())
            .finish()
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_loop_count_thing() {
        let mut b: u8 = 0;
        b <<= 1;
        b <<= 1;
        b |= 1;
        b <<= 1;
        b |= 1;
        b <<= 1;
        b |= 1;
        b <<= 1;
        b <<= 1;
        b |= 1;
        b <<= 1;
        b <<= 1;
        let x = f64::from(b);

        println!("x: {}", x);

        let tmp = (((234 as u16) + (134 as u16)) & 0xFF) as u8;
        println!("tmp: {}", tmp);
    }
}
