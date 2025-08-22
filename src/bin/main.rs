#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::delay::Delay;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::spi::master::Spi;
use esp_hal::spi::master::Config as SpiConfig;
use esp_hal::time::{Duration, Instant, Rate};
use log::info;
use core::default::Default;
use esp_hal::gpio::{self, Output, OutputConfig, Level};

use embedded_hal_bus::spi::ExclusiveDevice;

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, Primitive, PrimitiveStyle, Triangle},
};
use esp_hal::spi::Mode;

// Provides the parallel port and display interface builders
use mipidsi::interface::SpiInterface;
use mipidsi::options::Rotation;

// Provides the Display builder
use mipidsi::{models::ST7789, Builder};
use mipidsi::options::Orientation;

extern crate alloc;


// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    // generator version: 0.5.0

    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);


    let mut rtc = Rtc::new(peripherals.LPWR);
    let timer_group0 = TimerGroup::new(peripherals.TIMG0);
    let mut wdt0 = timer_group0.wdt;
    let timer_group1 = TimerGroup::new(peripherals.TIMG1);
    let mut wdt1 = timer_group1.wdt;
    rtc.swd.disable();
    rtc.rwdt.disable();
    wdt0.disable();
    wdt1.disable();
    let mut delay = Delay::new();

    let dc = Output::new(peripherals.GPIO15, Level::Low, OutputConfig::default());
    // Define the reset pin as digital outputs and make it high
    let mut rst = Output::new(peripherals.GPIO21, Level::Low, OutputConfig::default());
    rst.set_high();

    let mut bl = Output::new(peripherals.GPIO22, Level::Low, OutputConfig::default());
    bl.set_high();

    // Define the SPI pins and create the SPI interface
    let sck = peripherals.GPIO7;
    let miso = gpio::NoPin;
    let mosi = peripherals.GPIO6;
    let cs = peripherals.GPIO14;
    let spi_cfg = SpiConfig::default()
        .with_frequency(Rate::from_mhz(12))
        .with_mode(Mode::_0);
    let mut spi = Spi::new(peripherals.SPI2, spi_cfg).unwrap();
    let spi = spi
        .with_sck(sck)
        .with_mosi(mosi)
        .with_miso(miso);

    let cs_output = Output::new(cs, Level::High, OutputConfig::default());
    let spi_device = ExclusiveDevice::new_no_delay(spi, cs_output).unwrap();

    let mut buffer = [0_u8; 512];

    // Define the display interface with no chip select
    let di = SpiInterface::new(spi_device, dc, &mut buffer);

    // Define the display from the display interface and initialize it
    let mut display = Builder::new(ST7789, di)
        .display_size(172, 320)
        .orientation(Orientation::new().rotate(Rotation::Deg0))
        .set_offset(34, 0)
        .reset_pin(rst)
        .init(&mut delay)
        .unwrap();

    // Make the display all black
    display.clear(Rgb565::WHITE).unwrap();

    // Draw a smiley face with white eyes and a red mouth

    loop {
        // Draw an upside down red triangle to represent a smiling mouth
        Triangle::new(
            Point::new(30, 140),
            Point::new(60, 160),
            Point::new(130, 130),
        )
            .into_styled(PrimitiveStyle::with_fill(Rgb565::RED))
            .draw(&mut display).ok();
            // .draw(display)?;
        info!("Hello world!");
        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(500) {}
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}

// fn draw_smiley<D>(display: &mut D) -> Result<(), core::convert::Infallible>
// where
//     D: embedded_graphics::draw_target::DrawTarget<Color = Rgb565>,
// {
//     use embedded_graphics::pixelcolor::RgbColor;
//     use embedded_graphics::primitives::{Circle, PrimitiveStyle, Triangle};
//     use embedded_graphics::prelude::*;
//
//     // Face
//     Circle::new(Point::new(86, 160), 150)
//         .into_styled(PrimitiveStyle::with_fill(Rgb565::YELLOW))
//         .draw(display)?;
//
//     // Eyes
//     Circle::new(Point::new(60, 120), 15)
//         .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
//         .draw(display)?;
//     Circle::new(Point::new(112, 120), 15)
//         .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
//         .draw(display)?;
//
//     // Mouth (triangle)
//     Triangle::new(Point::new(60, 190), Point::new(112, 190), Point::new(86, 210))
//         .into_styled(PrimitiveStyle::with_fill(Rgb565::RED))
//         .draw(display)?;
//
//     Ok(())
// }
