pub mod mcp4725;
pub mod pca9685;

use std::ffi::CStr;
use std::fmt;
use std::os::raw::{c_char, c_int, c_uint};

use self::mcp4725::{MCP4725, MCP4725_DEFAULT_ADDRESS};
use self::pca9685::{PCA9685, PCA9685_DEFAULT_ADDRESS};

#[allow(non_camel_case_types)]
type int = c_int;
#[allow(non_camel_case_types)]
type unsigned = c_uint;

// #[cfg(target_arch = "arm")]
#[link(name = "pigpiod_if2", kind = "dylib")]
extern "C" {
    fn pigpio_start(addr: *const c_char, port: *const c_char) -> int;
    // fn pigpio_stop(pi: int);
    fn set_mode(pi: int, gpio: unsigned, mode: unsigned) -> int;
    fn gpio_write(pi: int, gpio: unsigned, level: unsigned) -> int;
    fn hardware_PWM(pi: int, gpio: unsigned, pwm_freq: unsigned, pwm_duty: u32) -> int;
    fn pigpio_error(code: int) -> *const c_char;
    fn i2c_open(pi: int, i2c_bus: unsigned, i2c_addr: unsigned, i2c_flags: unsigned) -> int;
    fn i2c_close(pi: int, handle: unsigned) -> int;
    // fn i2c_write_byte(pi: int, handle: unsigned, byte: unsigned) -> int;
    fn i2c_write_device(pi: int, handle: unsigned, buf: *const u8, count: unsigned) -> int;
    fn i2c_read_device(pi: int, handle: unsigned, buf: *mut u8, count: unsigned) -> int;
}

// /// HV507 polarity
// /// Pin 32 - BCM 12 (PWM0)
// static POLARITY_PIN: u8 = 12;

// /// High voltage converter "analog" signal
// /// Pin 33 - BCM 13 (PWM1)
// static VOLTAGE_PIN: u8 = 13;

pub enum GpioPin {
    /// HV507 blank
    /// Physical pin 11 - BCM 17
    Blank = 17,

    /// HV507 latch enable
    /// Physical pin 27 - BCM 13
    LatchEnable = 13,

    /// HV507 clock
    /// Physical pin 22 - BCM 15
    Clock = 15,

    /// HV507 data
    /// Physical pin 23 - BCM 16
    Data = 16,
}

// numbers taken from pigpio.h
pub enum GpioMode {
    Input = 0,
    Output = 1,
    Alt0 = 4,
    Alt1 = 5,
    Alt2 = 6,
    Alt3 = 7,
    Alt4 = 3,
    Alt5 = 2,
}

#[derive(Debug)]
pub struct PiError {
    msg: String,
    code: i32,
}

impl PiError {
    fn from_code(code: i32) -> PiError {
        assert!(code < 0);
        let msg_buf = unsafe { CStr::from_ptr(pigpio_error(code)) };
        let msg = msg_buf.to_str().unwrap().into();
        PiError { msg, code }
    }
}

impl fmt::Display for PiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Pi error code {}: '{}'", self.code, self.msg)
    }
}

impl ::std::error::Error for PiError {
    fn description(&self) -> &str {
        &self.msg
    }
}

pub type Result<T> = ::std::result::Result<T, PiError>;

macro_rules! res {
    ($code:expr, $ok:expr) => {
        if $code >= 0 {
            Ok($ok)
        } else {
            Err(PiError::from_code($code))
        }
    };
    ($code:expr) => {
        res!($code, ())
    };
}

pub struct RaspberryPi {
    pi_num: i32,
    pub mcp4725: MCP4725,
    pub pca9685: PCA9685,
}

impl RaspberryPi {
    pub fn new() -> Result<RaspberryPi> {
        let pi_num = {
            let null = ::std::ptr::null();
            let r = unsafe { pigpio_start(null, null) };
            res!(r, r)?
        };

        let mcp4725 = {
            let i2c = I2cHandle::new(pi_num, MCP4725_DEFAULT_ADDRESS)?;
            MCP4725::new(i2c)
        };

        let pca9685 = {
            let i2c = I2cHandle::new(pi_num, PCA9685_DEFAULT_ADDRESS)?;
            PCA9685::new(i2c)?
        };

        res!(pi_num, {
            RaspberryPi {
                pi_num,
                mcp4725,
                pca9685,
            }
        })
    }

    pub fn gpio_write(&mut self, gpio: GpioPin, level: u8) -> Result<()> {
        self.gpio_write_num(gpio as u32, level)
    }

    pub fn gpio_write_num(&mut self, gpio: u32, level: u8) -> Result<()> {
        let code = unsafe { gpio_write(self.pi_num, gpio, level as u32) };
        res!(code)
    }

    pub fn gpio_set_mode(&mut self, gpio: GpioPin, mode: GpioMode) -> Result<()> {
        let code = unsafe { set_mode(self.pi_num, gpio as u32, mode as u32) };
        res!(code)
    }

    pub fn set_pwm(&mut self, gpio: u32, pwm_freq: u32, pwm_duty: u32) -> Result<()> {
        let code = unsafe { hardware_PWM(self.pi_num, gpio, pwm_freq, pwm_duty) };
        res!(code)
    }


}

#[derive(Debug)]
pub struct I2cHandle {
    pi_num: i32,
    handle: u32,
}

impl I2cHandle {
    fn new_with_bus_and_flags(pi_num: i32, bus: u8, address: u16, flags: u8) -> Result<I2cHandle> {
        let handle_result = unsafe { i2c_open(pi_num, bus as u32, address as u32, flags as u32) };
        let handle = res!(handle_result, handle_result)? as u32;
        Ok(I2cHandle { pi_num, handle })
    }

    fn new(pi_num: i32, address: u16) -> Result<I2cHandle> {
        let bus = 1;
        let flags = 0;
        I2cHandle::new_with_bus_and_flags(pi_num, bus, address, flags)
    }

    pub fn write(&mut self, buf: &[u8]) -> Result<()> {
        res!(unsafe { i2c_write_device(self.pi_num, self.handle, buf.as_ptr(), buf.len() as u32) })
    }

    pub fn read(&mut self, count: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0; count];
        self.read_into(&mut buf)?;
        Ok(buf)
    }

    pub fn read_into(&mut self, buf: &mut [u8]) -> Result<()> {
        let read_result = unsafe {
            i2c_read_device(self.pi_num, self.handle, buf.as_mut_ptr(), buf.len() as u32)
        };
        let n_read = res!(read_result, read_result)?;
        assert!(n_read as usize == buf.len());
        Ok(())
    }
}

impl Drop for I2cHandle {
    fn drop(&mut self) {
        let result = res!(unsafe { i2c_close(self.pi_num, self.handle) });
        match result {
            Ok(()) => debug!("Successfully dropped {:#?}", self),
            Err(err) => error!("Error while dropping {:#?}: {:#?}", self, err),
        }
    }
}
