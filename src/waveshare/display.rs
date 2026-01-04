mod font;

use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::gpio::{Gpio9, Gpio10, Gpio11, Gpio12, Gpio13, Gpio14, Gpio46, Input, Output, PinDriver};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::spi::{SpiDeviceDriver, SpiDriver, SpiDriverConfig, config::Config as SpiConfig};
use esp_idf_hal::units::FromValueType;
use log::info;
use font::FONT;


/// Display command constants for the Waveshare 4.2" e-Paper display
#[allow(dead_code)]
mod commands {
    pub const PANEL_SETTING: u8 = 0x00;
    pub const POWER_SETTING: u8 = 0x01;
    pub const POWER_OFF: u8 = 0x02;
    pub const POWER_ON: u8 = 0x04;
    pub const BOOSTER_SOFT_START: u8 = 0x06;
    pub const DEEP_SLEEP: u8 = 0x07;
    pub const DATA_START_TRANSMISSION_1: u8 = 0x10;
    pub const DATA_STOP_TRANSMISSION: u8 = 0x11;
    pub const DISPLAY_REFRESH: u8 = 0x12;
    pub const DATA_START_TRANSMISSION_2: u8 = 0x13; // Red Pixel Data
    pub const LUT_FOR_VCOM: u8 = 0x20;
    pub const LUT_WHITE_TO_WHITE: u8 = 0x21;
    pub const LUT_BLACK_TO_WHITE: u8 = 0x22;
    pub const LUT_WHITE_TO_BLACK: u8 = 0x23;
    pub const LUT_BLACK_TO_BLACK: u8 = 0x24;
    pub const PLL_CONTROL: u8 = 0x30;
    pub const VCOM_AND_DATA_INTERVAL_SETTING: u8 = 0x50;
    pub const RESOLUTION_SETTING: u8 = 0x61;
    pub const VCM_DC_SETTING: u8 = 0x82;
}

/// Lookup table for display waveforms
const LUT_ALL: [u8; 233] = [
    0x01, 0x0A, 0x1B, 0x0F, 0x03, 0x01, 0x01,
    0x05, 0x0A, 0x01, 0x0A, 0x01, 0x01, 0x01,
    0x05, 0x08, 0x03, 0x02, 0x04, 0x01, 0x01,
    0x01, 0x04, 0x04, 0x02, 0x00, 0x01, 0x01,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01,
    0x01, 0x0A, 0x1B, 0x0F, 0x03, 0x01, 0x01,
    0x05, 0x4A, 0x01, 0x8A, 0x01, 0x01, 0x01,
    0x05, 0x48, 0x03, 0x82, 0x84, 0x01, 0x01,
    0x01, 0x84, 0x84, 0x82, 0x00, 0x01, 0x01,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01,
    0x01, 0x0A, 0x1B, 0x8F, 0x03, 0x01, 0x01,
    0x05, 0x4A, 0x01, 0x8A, 0x01, 0x01, 0x01,
    0x05, 0x48, 0x83, 0x82, 0x04, 0x01, 0x01,
    0x01, 0x04, 0x04, 0x02, 0x00, 0x01, 0x01,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01,
    0x01, 0x8A, 0x1B, 0x8F, 0x03, 0x01, 0x01,
    0x05, 0x4A, 0x01, 0x8A, 0x01, 0x01, 0x01,
    0x05, 0x48, 0x83, 0x02, 0x04, 0x01, 0x01,
    0x01, 0x04, 0x04, 0x02, 0x00, 0x01, 0x01,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01,
    0x01, 0x8A, 0x9B, 0x8F, 0x03, 0x01, 0x01,
    0x05, 0x4A, 0x01, 0x8A, 0x01, 0x01, 0x01,
    0x05, 0x48, 0x03, 0x42, 0x04, 0x01, 0x01,
    0x01, 0x04, 0x04, 0x42, 0x00, 0x01, 0x01,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x02, 0x00, 0x00, 0x07, 0x17, 0x41, 0xA8,
    0x32, 0x30,
];

/// E-Paper Display driver for Waveshare 4.2" display
pub struct Epd<'a> {
    spi: SpiDeviceDriver<'a, SpiDriver<'a>>,
    cs_pin: PinDriver<'a, Gpio10, Output>,
    dc_pin: PinDriver<'a, Gpio9, Output>,
    reset_pin: PinDriver<'a, Gpio13, Output>,
    busy_pin: PinDriver<'a, Gpio14, Input>,
    width: u32,
    height: u32,
}

impl<'a> Epd<'a> {
    /// Create a new EPD instance with the given peripherals
    pub fn new(
        spi: SpiDeviceDriver<'a, SpiDriver<'a>>,
        cs_pin: PinDriver<'a, Gpio10, Output>,
        dc_pin: PinDriver<'a, Gpio9, Output>,
        reset_pin: PinDriver<'a, Gpio13, Output>,
        busy_pin: PinDriver<'a, Gpio14, Input>,
    ) -> Self {
        let mut epd = Self {
            spi,
            cs_pin,
            dc_pin,
            reset_pin,
            busy_pin,
            width: 400,
            height: 300,
        };

        info!("SPI Bus Initialized successfully!");

        // Initial power setup
        epd.send_command(commands::POWER_SETTING);
        epd.send_command(commands::PLL_CONTROL);
        epd.send_data(0x3C);
        epd.send_command(commands::POWER_ON);

        epd
    }

    /// Hardware reset the display
    pub fn reset(&mut self) {
        self.reset_pin.set_high().unwrap();
        FreeRtos::delay_ms(100);
        self.reset_pin.set_low().unwrap();
        FreeRtos::delay_ms(2);
        self.reset_pin.set_high().unwrap();
        FreeRtos::delay_ms(100);
    }

    /// Send a command byte to the display
    pub fn send_command(&mut self, command: u8) {
        self.dc_pin.set_low().unwrap();
        self.cs_pin.set_low().unwrap();
        self.spi.write(&[command]).unwrap();
        self.cs_pin.set_high().unwrap();
    }

    /// Send a single data byte to the display
    pub fn send_data(&mut self, data: u8) {
        self.dc_pin.set_high().unwrap();
        self.cs_pin.set_low().unwrap();
        self.spi.write(&[data]).unwrap();
        self.cs_pin.set_high().unwrap();
    }

    /// Send multiple data bytes to the display
    pub fn send_data_bulk(&mut self, data: &[u8]) {
        self.dc_pin.set_high().unwrap();
        self.cs_pin.set_low().unwrap();
        self.spi.write(data).unwrap();
        self.cs_pin.set_high().unwrap();
    }

    /// Wait until the display is not busy
    pub fn read_busy(&self) {
        while self.busy_pin.is_high() {
            FreeRtos::delay_ms(20);
        }
    }

    /// Turn on the display (normal mode)
    pub fn turn_on_display(&mut self) {
        self.send_command(0x22); // Display Update Control
        self.send_data(0xF7);
        self.send_command(0x20); // Activate Display Update Sequence
        self.read_busy();
    }

    /// Turn on the display (fast mode)
    #[allow(dead_code)]
    pub fn turn_on_display_fast(&mut self) {
        self.send_command(0x22); // Display Update Control
        self.send_data(0xC7);
        self.send_command(0x20); // Activate Display Update Sequence
        self.read_busy();
    }

    /// Turn on the display (partial refresh mode)
    pub fn turn_on_display_partial(&mut self) {
        self.send_command(0x22); // Display Update Control
        self.send_data(0xFF);
        self.send_command(0x20); // Activate Display Update Sequence
        self.read_busy();
    }

    /// Turn on the display (4-gray mode)
    #[allow(dead_code)]
    pub fn turn_on_display_4gray(&mut self) {
        self.send_command(0x22); // Display Update Control
        self.send_data(0xCF);
        self.send_command(0x20); // Activate Display Update Sequence
        self.read_busy();
    }

    /// Initialize the display
    pub fn init(&mut self) {
        // EPD hardware init start
        self.reset();
        self.read_busy();

        self.send_command(commands::DISPLAY_REFRESH); // SWRESET
        self.read_busy();

        // Display update control
        self.send_command(0x21);
        self.send_data(0x40);
        self.send_data(0x00);

        // BorderWaveform
        self.send_command(0x3C);
        self.send_data(0x05);

        // Data entry mode
        self.send_command(0x11);
        self.send_data(0x03); // X-mode

        self.send_command(0x44);
        self.send_data(0x00);
        self.send_data(0x31);

        self.send_command(0x45);
        self.send_data(0x00);
        self.send_data(0x00);
        self.send_data(0x2B);
        self.send_data(0x01);

        self.send_command(0x4E);
        self.send_data(0x00);

        self.send_command(0x4F);
        self.send_data(0x00);
        self.send_data(0x00);
        self.read_busy();
    }

    /// Load the lookup table for display waveforms
    #[allow(dead_code)]
    pub fn lut(&mut self) {
        self.send_command(0x32);
        for i in 0..227 {
            self.send_data(LUT_ALL[i]);
        }

        self.send_command(0x3F);
        self.send_data(LUT_ALL[227]);

        self.send_command(0x03);
        self.send_data(LUT_ALL[228]);

        self.send_command(0x04);
        self.send_data(LUT_ALL[229]);
        self.send_data(LUT_ALL[230]);
        self.send_data(LUT_ALL[231]);

        self.send_command(0x2C);
        self.send_data(LUT_ALL[232]);
    }

    /// Get the display buffer size in bytes
    pub fn buffer_size(&self) -> usize {
        (self.width as usize * self.height as usize) / 8
    }

    /// Clear the display (fill with white)
    pub fn clear(&mut self) {
        let linewidth = if self.width % 8 == 0 {
            self.width / 8
        } else {
            self.width / 8 + 1
        };

        let buf_size = (self.height * linewidth) as usize;
        let white_buf = vec![0xFF_u8; buf_size];

        self.send_command(0x24);
        self.send_data_bulk(&white_buf);

        self.send_command(0x26);
        self.send_data_bulk(&white_buf);

        self.turn_on_display();
    }

    /// Display an image buffer on the screen
    pub fn display(&mut self, image: &[u8]) {
        self.send_command(0x24);
        self.send_data_bulk(image);

        self.send_command(0x26);
        self.send_data_bulk(image);

        self.turn_on_display();
    }

    /// Partial display update
    #[allow(dead_code)]
    pub fn display_partial(&mut self, image: &[u8]) {
        self.send_command(0x3C); // BorderWaveform
        self.send_data(0x80);

        self.send_command(0x21); // Display update control
        self.send_data(0x00);
        self.send_data(0x00);

        self.send_command(0x3C); // BorderWaveform
        self.send_data(0x80);

        self.send_command(0x44);
        self.send_data(0x00);
        self.send_data(0x31);

        self.send_command(0x45);
        self.send_data(0x00);
        self.send_data(0x00);
        self.send_data(0x2B);
        self.send_data(0x01);

        self.send_command(0x4E);
        self.send_data(0x00);

        self.send_command(0x4F);
        self.send_data(0x00);
        self.send_data(0x00);

        self.send_command(0x24); // WRITE_RAM
        self.send_data_bulk(image);
        self.turn_on_display_partial();
    }

    /// Put the display into deep sleep mode
    pub fn sleep(&mut self) {
        self.send_command(commands::DEEP_SLEEP);
        self.send_data(0x01);
        FreeRtos::delay_ms(2000);
    }

    /// Get display width
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Get display height
    pub fn height(&self) -> u32 {
        self.height
    }
}

/// Simple framebuffer for MONO_HLSB format
pub struct FrameBuffer {
    buffer: Vec<u8>,
    width: u32,
    height: u32,
}

impl FrameBuffer {
    /// Create a new framebuffer
    pub fn new(width: u32, height: u32) -> Self {
        let size = (width * height / 8) as usize;
        Self {
            buffer: vec![0xFF; size], // Initialize with white
            width,
            height,
        }
    }

    /// Fill the entire buffer with a color (0 = black, 1 = white)
    pub fn fill(&mut self, color: u8) {
        let fill_byte = if color == 0 { 0x00 } else { 0xFF };
        for byte in &mut self.buffer {
            *byte = fill_byte;
        }
    }

    /// Set a single pixel
    pub fn pixel(&mut self, x: u32, y: u32, color: u8) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = ((x + y * self.width) / 8) as usize;
        let bit = 0x80 >> (x % 8);
        if color == 0 {
            self.buffer[idx] &= !bit; // Black
        } else {
            self.buffer[idx] |= bit; // White
        }
    }

    /// Draw a horizontal line
    pub fn hline(&mut self, x: u32, y: u32, length: u32, color: u8) {
        for i in 0..length {
            self.pixel(x + i, y, color);
        }
    }

    /// Draw a vertical line
    pub fn vline(&mut self, x: u32, y: u32, length: u32, color: u8) {
        for i in 0..length {
            self.pixel(x, y + i, color);
        }
    }

    /// Draw a line using Bresenham's algorithm
    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u8) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;

        loop {
            if x >= 0 && y >= 0 {
                self.pixel(x as u32, y as u32, color);
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Draw a rectangle outline
    pub fn rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u8) {
        self.hline(x, y, w, color);
        self.hline(x, y + h - 1, w, color);
        self.vline(x, y, h, color);
        self.vline(x + w - 1, y, h, color);
    }

    /// Draw a filled rectangle
    pub fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u8) {
        for dy in 0..h {
            for dx in 0..w {
                self.pixel(x + dx, y + dy, color);
            }
        }
    }

    /// Draw text using a simple 8x8 font (basic ASCII)
    pub fn text(&mut self, s: &str, x: u32, y: u32, color: u8) {
        // Simple 8x8 font for basic characters
        // const FONT: [[u8; 8]; 96] = include!("font.rs");

        let mut cx = x;
        for ch in s.chars() {
            let idx = ch as usize;
            if idx >= 32 && idx < 128 {
                let glyph = &FONT[idx - 32];
                for (row, &bits) in glyph.iter().enumerate() {
                    for col in 0..8 {
                        if bits & (1 << col) != 0 {
                            self.pixel(cx + col, y + row as u32, color);
                        }
                    }
                }
            }
            cx += 8;
        }
    }

    /// Get the raw buffer
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }
}

fn main() -> anyhow::Result<()> {
    // Initialize ESP-IDF
    esp_idf_svc::sys::link_patches();
    esp_idf_logger::init().unwrap();

    info!("Initializing e-Paper display...");

    let peripherals = Peripherals::take()?;

    // SPI pins
    let sck = peripherals.pins.gpio12;
    let mosi = peripherals.pins.gpio11;
    let miso = peripherals.pins.gpio46;

    // Control pins
    let cs_pin = PinDriver::output(peripherals.pins.gpio10)?;
    let dc_pin = PinDriver::output(peripherals.pins.gpio9)?;
    let reset_pin = PinDriver::output(peripherals.pins.gpio13)?;
    let busy_pin = PinDriver::input(peripherals.pins.gpio14)?;

    // Initialize SPI
    let spi_driver = SpiDriver::new(
        peripherals.spi2,
        sck,
        mosi,
        Some(miso),
        &SpiDriverConfig::default(),
    )?;

    let spi_config = SpiConfig::new()
        .baudrate(20.MHz().into());

    let spi_device = SpiDeviceDriver::new(spi_driver, None::<Gpio10>, &spi_config)?;

    // Create EPD instance
    let mut epd = Epd::new(spi_device, cs_pin, dc_pin, reset_pin, busy_pin);

    info!("Resetting the screen...");
    epd.init();
    epd.clear();

    // Create framebuffer
    let mut fb = FrameBuffer::new(epd.width(), epd.height());
    info!("Created buffer of size: {} bytes", fb.buffer().len());

    const BLACK: u8 = 0x00;
    const WHITE: u8 = 0x01;

    info!("Displaying 'Hello World!' on the screen...");
    fb.fill(WHITE);
    fb.text("Hello Rust World", 30, 10, BLACK);
    fb.pixel(30, 10, BLACK);
    fb.hline(30, 30, 10, BLACK);
    fb.vline(30, 50, 10, BLACK);
    fb.line(30, 70, 40, 80, BLACK);
    fb.rect(30, 90, 10, 10, BLACK);
    fb.fill_rect(30, 110, 10, 10, BLACK);

    for row in 0..36 {
        let row_str = row.to_string();
        fb.text(&row_str, 0, row * 8, BLACK);
    }
    fb.text("Line 36", 0, 288, BLACK);

    info!("Buffer content (first 512 bytes): {:?}", &fb.buffer()[..512]);

    epd.display(fb.buffer());
    epd.sleep();

    info!("Display complete!");

    Ok(())
}
