#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

extern crate alloc;

use esp_backtrace as _;
use alloc::vec::Vec;
use esp_hal::clock::CpuClock;
use core::sync::atomic::AtomicBool;
use core::cell::RefCell;
static PREV: AtomicBool = AtomicBool::new(false);
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig, Input, InputConfig};
use esp_hal::main;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::spi::Mode;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use log::info;
use embedded_hal_bus::spi::RefCellDevice;
use embedded_sdmmc::{SdCard, VolumeManager, VolumeIdx, TimeSource, Timestamp, Mode as SdMode, ShortFileName};

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    image::{Image, ImageRawLE},
};

use mipidsi::{
    interface::SpiInterface,
    models::ST7789,
    options::{ColorInversion, ColorOrder, Orientation, Rotation},
    Builder,
};

// Minimal time source for embedded-sdmmc (we don't care about real timestamps now)
struct DummyTime;
impl TimeSource for DummyTime {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp::from_fat(0, 0)
    }
}

// RAW movie geometry for 172x320 RGB565LE frames
const RAW_W: u32 = 172;
const RAW_H: u32 = 320;
const FRAME_SZ: usize = (RAW_W as usize) * (RAW_H as usize) * 2; // bytes

// Single frame buffer (~110,080 bytes)
static mut MOV_FRAMEBUF: [u8; FRAME_SZ] = [0u8; FRAME_SZ];

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

    // Define the SPI pins and create the shared SPI interface
    let sck = peripherals.GPIO7;
    let mosi = peripherals.GPIO6;
    let miso = peripherals.GPIO5; // SD needs MISO

    // Use a moderate frequency that works for both SD init and LCD
    let spi_cfg = SpiConfig::default()
        .with_frequency(Rate::from_mhz(12))
        .with_mode(Mode::_0);

    let spi_raw = Spi::new(peripherals.SPI2, spi_cfg).unwrap()
        .with_sck(sck)
        .with_mosi(mosi)
        .with_miso(miso);

    let spi_bus = RefCell::new(spi_raw);
    let sd_dev_delay = Delay::new();

    // ---------------- SD card (SPI) ----------------
    // SD CS on GPIO4 per board pinout; use an ExclusiveDevice on the same SPI2 bus
    let sd_cs = Output::new(peripherals.GPIO4, Level::High, OutputConfig::default());
    let sd_dev = RefCellDevice::new(&spi_bus, sd_cs, sd_dev_delay).unwrap();
    let mut sd_delay_for_card = Delay::new();

    // Initialize SD card (SPI mode). `SdCard::new` returns the card directly (not a Result).
    // Probe capacity (may return an error if no card inserted), then release the bus for LCD use.
    let sd = SdCard::new(sd_dev, &mut sd_delay_for_card);
    match sd.num_bytes() {
        Ok(size) => log::info!("SD size: {} bytes", size),
        Err(_) => log::warn!("SD: failed to read size (no card?)"),
    }

    // Prepare FAT volume manager (keep it alive for movie playback)
    let mut volume_mgr = VolumeManager::new(sd, DummyTime);
    let mut volume = match volume_mgr.open_volume(VolumeIdx(0)) {
        Ok(v) => v,
        Err(err) => { info!("open_volume failed: {:?}", err); loop { delay.delay_millis(1000); } }
    };

    // ---------------- LCD (SPI) ----------------
    // Create a new ExclusiveDevice for the LCD on CS GPIO14 and reuse the same SPI bus.
    let lcd_dev_delay = Delay::new();
    let lcd_cs = Output::new(peripherals.GPIO14, Level::High, OutputConfig::default());
    let lcd_dev = RefCellDevice::new(&spi_bus, lcd_cs, lcd_dev_delay).unwrap();

    let mut buffer = [0_u8; 512];

    // Define the display interface with LCD device + DC
    let di = SpiInterface::new(lcd_dev, dc, &mut buffer);

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

    // Clear the display
    display.clear(Rgb565::BLACK).unwrap();

    // Open root dir and choose a movie file: prefer names starting with "NO" (e.g., NO_COW...), else first .RAW
    let mut root_dir = match volume.open_root_dir() {
        Ok(d) => d,
        Err(err) => { info!("open_root_dir failed: {:?}", err); loop { delay.delay_millis(1000); } }
    };

    // Build a playlist of all valid .RAW files in root
    let mut movies: Vec<ShortFileName> = Vec::new();
    let _ = root_dir.iterate_dir(|e| {
        if !e.attributes.is_directory() && e.name.extension() == b"RAW" {
            let base = e.name.base_name();
            let sz = e.size as usize;
            if base.starts_with(b"_") {
                info!("Skipping AppleDouble/hidden: {} ({} bytes)", e.name, e.size);
            } else if sz < FRAME_SZ {
                info!("Skipping too-small RAW: {} ({} bytes < one frame)", e.name, e.size);
            } else {
                movies.push(e.name.clone());
            }
        }
    });
    info!("Found {} movie(s)", movies.len());

    if movies.is_empty() {
        info!("No .RAW movie found in root.");
        loop { delay.delay_millis(1000); }
    }

    // Prefer an entry whose base name starts with "NO"; otherwise start at 0
    let mut idx: usize = 0;
    for (i, name) in movies.iter().enumerate() {
        if name.base_name().starts_with(b"NO") { idx = i; break; }
    }

    info!("Playing movie: {} ({}x{} RGB565)", movies[idx], RAW_W, RAW_H);
    loop {
        // Open current movie by short name
        if let Ok(mut file) = root_dir.open_file_in_dir(&movies[idx], SdMode::ReadOnly) {
            let mut advance = false;
            loop {
                let n = match file.read(unsafe { &mut MOV_FRAMEBUF[..] }) {
                    Ok(n) => n,
                    Err(err) => { info!("Read error: {:?}", err); break; }
                };
                if n == 0 { break; }
                if n < FRAME_SZ { info!("Short/invalid frame chunk ({} bytes), skipping movie", n); advance = true; break; }

                let raw = ImageRawLE::<Rgb565>::new(unsafe { &MOV_FRAMEBUF[..FRAME_SZ] }, RAW_W);
                let _ = Image::new(&raw, Point::new(0, 0)).draw(&mut display);

                // BOOT edge detect (active-low)
                let pressed = boot_btn.is_low();
                let was_pressed = PREV.swap(pressed, core::sync::atomic::Ordering::Relaxed);
                if pressed && !was_pressed {
                    info!("BOOT pressed");
                    advance = true; // switch to next movie
                } else if !pressed && was_pressed {
                    info!("BOOT released");
                }

                // crude frame pacing; adjust as needed (also acts as a tiny debounce)
                delay.delay_millis(3);
                if advance { break; }
            }
            let _ = file.close();

            if advance {
                idx = (idx + 1) % movies.len();
                info!("Next movie: {}", movies[idx]);
            }
        } else {
            info!("open_file_in_dir failed; sleeping forever");
            loop { delay.delay_millis(1000); }
        }
        // Loop: reopen current (or next) movie
    }
}
