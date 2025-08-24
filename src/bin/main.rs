#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use core::sync::atomic::{AtomicBool, Ordering};
static PREV: AtomicBool = AtomicBool::new(false);
use esp_hal::delay::Delay;
use esp_hal::gpio::{self, Level, Output, OutputConfig, Input, InputConfig};
use esp_hal::main;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::spi::Mode;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use log::info;

use embedded_graphics::{
    pixelcolor::{Rgb565},
    prelude::*,
    image::{Image, ImageRawLE},
};

use embedded_hal_bus::spi::ExclusiveDevice;

use mipidsi::{
    interface::SpiInterface,
    models::ST7789,
    options::{ColorInversion, ColorOrder, Orientation, Rotation},
    Builder,
};

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

    let boot_btn = Input::new(peripherals.GPIO9, InputConfig::default());

    // TODO DMA access for fast write
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
        .with_frequency(Rate::from_mhz(80))
        .with_mode(Mode::_0);
    let spi = Spi::new(peripherals.SPI2, spi_cfg).unwrap();
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
        .color_order(ColorOrder::Rgb)
        .invert_colors(ColorInversion::Inverted)
        .display_offset(34, 0)
        .reset_pin(rst)
        .init(&mut delay)
        .unwrap();

    // Make the display all white
    display.clear(Rgb565::BLACK).unwrap();
    const RAW_W: u32 = 172;
    const RAW_H: u32 = 320;

    // Two images: toggle with BOOT button
    let pic1: &[u8] = include_bytes!("../../assets/rgb/pic_1_172x320.rgb565");
    let pic2: &[u8] = include_bytes!("../../assets/rgb/pic_2_172x320.rgb565");
    let raw1 = ImageRawLE::<Rgb565>::new(pic1, RAW_W);
    let raw2 = ImageRawLE::<Rgb565>::new(pic2, RAW_W);
    let mut img_idx: usize = 0;

    // Draw initial image
    Image::new(&raw1, Point::new(0, 0))
        .draw(&mut display)
        .unwrap();

    loop {
        // Poll BOOT (GPIO9). Active-low on most boards.
        let pressed = boot_btn.is_low();
        let was_pressed = PREV.load(Ordering::Relaxed);
        if pressed && !was_pressed {
            info!("BOOT pressed");
            // Toggle image index and draw
            img_idx ^= 1;
            match img_idx {
                0 => { Image::new(&raw1, Point::new(0, 0)).draw(&mut display).ok(); }
                _ => { Image::new(&raw2, Point::new(0, 0)).draw(&mut display).ok(); }
            }
        }
        if !pressed && was_pressed {
            info!("BOOT released");
        }
        PREV.store(pressed, Ordering::Relaxed);

        // Also print a dot periodically so you see liveness
        delay.delay_millis(50);
    }
}
