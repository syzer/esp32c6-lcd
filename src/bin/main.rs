#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{self, Level, Output, OutputConfig};
use esp_hal::main;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::spi::Mode;
use esp_hal::time::{Duration, Instant, Rate};
use esp_hal::timer::timg::TimerGroup;
use log::info;

use embedded_graphics::{
    geometry::OriginDimensions,
    pixelcolor::{Rgb565, RgbColor},
    prelude::*,
    primitives::{Circle, PrimitiveStyle, Triangle},
};

use embedded_hal_bus::spi::ExclusiveDevice;

use mipidsi::{
    interface::SpiInterface,
    models::ST7789,
    options::{ColorInversion, ColorOrder, Orientation, Rotation},
    Builder,
};

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
        .orientation(Orientation::new().rotate(Rotation::Deg90))
        .color_order(ColorOrder::Rgb)
        .invert_colors(ColorInversion::Inverted)
        .display_offset(34, 0)
        .reset_pin(rst)
        .init(&mut delay)
        .unwrap();

    // Make the display all white
    display.clear(Rgb565::BLACK).unwrap();
    draw_smiley(&mut display);

    loop {
        // Draw a full smiley face using embedded-graphics primitives
        info!("Hello world!");
        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(1000) {}
    }

}

fn draw_smiley<D>(display: &mut D)
where
    D: embedded_graphics::draw_target::DrawTarget<Color = Rgb565> + OriginDimensions,
{
    let Size { width, height } = display.size();
    let (w, h) = (width as i32, height as i32);

    // Keep some margin so rounded glass corners don’t clip the face
    let margin = 14_i32;

    // Center and radius that fit BOTH 172×320 and rounded mask
    let cx = w / 2;
    let cy = h / 2 + (h - w).max(0) / 6; // nudge down a bit on tall screens
    let r  = (w.min(h) / 2) - margin;

    // Clear
    let _ = display.clear(Rgb565::new(200, 230, 255)); // light blue bg

    // Face
    let _ = Circle::new(Point::new(cx - r, cy - r), (2 * r) as u32)
        .into_styled(PrimitiveStyle::with_fill(Rgb565::YELLOW))
        .draw(display);

    // Eyes
    let eye_dx = r / 3;
    let eye_dy = -r / 5;
    let eye_r  = r / 10;
    for ex in [-eye_dx, eye_dx] {
        let _ = Circle::new(Point::new(cx + ex - eye_r, cy + eye_dy - eye_r), (2 * eye_r) as u32)
            .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
            .draw(display);
    }

    // Mouth (triangle smile)
    let mouth_w = (r as f32 * 1.2) as i32;
    let mouth_y = cy + r / 3;
    let mouth_h = r / 4;
    let _ = Triangle::new(
        Point::new(cx - mouth_w / 2, mouth_y),
        Point::new(cx + mouth_w / 2, mouth_y),
        Point::new(cx, mouth_y + mouth_h),
    )
        .into_styled(PrimitiveStyle::with_fill(Rgb565::RED))
        .draw(display);
}
