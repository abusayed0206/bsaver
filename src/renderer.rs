//! Font rendering using cosmic-text for proper Bangla text shaping
//!
//! Memory optimizations:
//! - Reusable text render buffer to avoid per-render allocations
//! - SwashCache with periodic cleanup to limit glyph memory
//! - Digit width caching to avoid re-measuring
//! - Static font system initialized once

use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Weight};
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::config::Config;

/// Embedded Ekush font data (~180KB)
const EKUSH_FONT: &[u8] = include_bytes!("../Ekush-Regular.ttf");

/// Maximum SwashCache render calls before cleanup (limits glyph memory growth)
const SWASH_CACHE_CLEANUP_INTERVAL: u32 = 500;

/// Font size constants for layout calculations
pub mod font_ratios {
    /// Character width ratio for time display
    pub const CHAR_WIDTH_RATIO: f32 = 0.6;
    /// Maximum height ratio for base font
    pub const MAX_HEIGHT_RATIO: f32 = 0.20;
    /// Time period font ratio relative to time font
    pub const PERIOD_RATIO: f32 = 0.28;
    /// Day font ratio relative to time font
    pub const DAY_RATIO: f32 = 0.22;
    /// Date font ratio relative to time font
    pub const DATE_RATIO: f32 = 0.18;
}

/// Global font system (initialized once)
static FONT_SYSTEM: OnceLock<std::sync::Mutex<FontSystem>> = OnceLock::new();

/// Initialize the font system with embedded Ekush font only (no system fonts)
fn get_font_system() -> &'static std::sync::Mutex<FontSystem> {
    FONT_SYSTEM.get_or_init(|| {
        // Create font system without loading system fonts (saves ~5MB)
        // Use Bangla locale since this is a Bangla screensaver
        let mut font_system = FontSystem::new_with_locale_and_db(
            "bn-BD".to_string(),
            cosmic_text::fontdb::Database::new(),
        );
        // Load only the embedded Ekush font
        font_system.db_mut().load_font_data(EKUSH_FONT.to_vec());
        std::sync::Mutex::new(font_system)
    })
}

/// Font renderer for the screensaver
pub struct Renderer {
    pub config: Config,
    swash_cache: SwashCache,
    /// Cached digit widths per font size (font_size_bits -> max_width)
    digit_width_cache: HashMap<u32, u32>,
    /// Counter for SwashCache cleanup
    render_count: u32,
}

impl Renderer {
    /// Create a new renderer with the embedded Bangla font
    pub fn new(config: Config) -> Self {
        // Ensure font system is initialized
        let _ = get_font_system();

        Self {
            config,
            swash_cache: SwashCache::new(),
            digit_width_cache: HashMap::with_capacity(8),
            render_count: 0,
        }
    }

    /// Periodically clean up SwashCache to limit memory growth
    fn maybe_cleanup_cache(&mut self) {
        self.render_count += 1;
        if self.render_count >= SWASH_CACHE_CLEANUP_INTERVAL {
            // Clear the glyph rasterization cache to free memory
            self.swash_cache = SwashCache::new();
            self.render_count = 0;
        }
    }

    /// Render text to a pixel buffer (BGRA format)
    /// Returns (width, height, pixels)
    /// Uses internal reusable buffer to reduce allocations
    pub fn render_text(&mut self, text: &str, font_size: f32) -> (u32, u32, Vec<u8>) {
        self.maybe_cleanup_cache();

        let mut font_system = get_font_system().lock().unwrap();

        // Use larger line height for Bangla vowel marks (ী, ি, ু, etc.)
        let metrics = Metrics::new(font_size, font_size * 1.6);
        let mut buffer = Buffer::new(&mut font_system, metrics);

        // Set a large width to prevent wrapping, and extra height for vowel marks
        buffer.set_size(&mut font_system, Some(2000.0), Some(font_size * 2.5));

        // Set text with advanced shaping for Bangla
        let attrs = Attrs::new()
            .family(Family::Name("Ekush"))
            .weight(Weight::NORMAL);

        buffer.set_text(&mut font_system, text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, true);

        // Calculate actual text dimensions
        let mut max_width: f32 = 0.0;
        let mut total_height: f32 = 0.0;

        for run in buffer.layout_runs() {
            max_width = max_width.max(run.line_w);
            total_height = total_height.max(run.line_y + run.line_height);
        }

        let width = (max_width.ceil() as u32).max(1);
        let height = (total_height.ceil() as u32).max(1);

        // Create pixel buffer (BGRA format)
        let mut pixels = vec![0u8; (width * height * 4) as usize];

        // Fill with background color but alpha = 0 (transparent for blitting)
        let bg = self.config.background_color;
        for i in 0..(width * height) as usize {
            pixels[i * 4] = bg[2]; // B
            pixels[i * 4 + 1] = bg[1]; // G
            pixels[i * 4 + 2] = bg[0]; // R
            pixels[i * 4 + 3] = 0; // A = 0 for transparent background
        }

        // Render text using cosmic-text's draw callback
        let text_color = Color::rgb(
            self.config.text_color[0],
            self.config.text_color[1],
            self.config.text_color[2],
        );

        buffer.draw(
            &mut font_system,
            &mut self.swash_cache,
            text_color,
            |x, y, w, h, color| {
                // Draw each pixel span
                for dy in 0..h {
                    for dx in 0..w {
                        let px = x + dx as i32;
                        let py = y + dy as i32;

                        if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                            let idx = ((py as u32 * width + px as u32) * 4) as usize;
                            if idx + 3 < pixels.len() {
                                let alpha = color.a() as f32 / 255.0;
                                if alpha > 0.0 {
                                    let inv_alpha = 1.0 - alpha;
                                    pixels[idx] = (color.b() as f32 * alpha
                                        + pixels[idx] as f32 * inv_alpha)
                                        as u8;
                                    pixels[idx + 1] = (color.g() as f32 * alpha
                                        + pixels[idx + 1] as f32 * inv_alpha)
                                        as u8;
                                    pixels[idx + 2] = (color.r() as f32 * alpha
                                        + pixels[idx + 2] as f32 * inv_alpha)
                                        as u8;
                                    pixels[idx + 3] = 255;
                                }
                            }
                        }
                    }
                }
            },
        );

        (width, height, pixels)
    }

    /// Render text centered at a position
    pub fn render_text_centered(
        &mut self,
        text: &str,
        font_size: f32,
        screen_width: u32,
        y_position: u32,
        screen_buffer: &mut [u8],
        screen_stride: u32,
    ) {
        let (text_width, text_height, text_pixels) = self.render_text(text, font_size);

        let x_start = if text_width < screen_width {
            ((screen_width - text_width) / 2) as i32
        } else {
            0
        };

        // Blit the text to the screen buffer
        for ty in 0..text_height {
            for tx in 0..text_width {
                let sx = x_start + tx as i32;
                let sy = y_position as i32 + ty as i32;

                if sx >= 0 && sx < screen_stride as i32 / 4 && sy >= 0 {
                    let src_idx = ((ty * text_width + tx) * 4) as usize;
                    let dst_idx = ((sy as u32 * screen_stride) + (sx as u32 * 4)) as usize;

                    if dst_idx + 3 < screen_buffer.len() && src_idx + 3 < text_pixels.len() {
                        // Only copy if there's actual content (not background)
                        let alpha = text_pixels[src_idx + 3];
                        if alpha > 0 {
                            screen_buffer[dst_idx] = text_pixels[src_idx];
                            screen_buffer[dst_idx + 1] = text_pixels[src_idx + 1];
                            screen_buffer[dst_idx + 2] = text_pixels[src_idx + 2];
                            screen_buffer[dst_idx + 3] = text_pixels[src_idx + 3];
                        }
                    }
                }
            }
        }
    }

    /// Render time with fixed-width digit cells to prevent shifting
    /// Each digit gets a fixed cell width, and colons get a smaller fixed width
    pub fn render_time_fixed_grid(
        &mut self,
        text: &str,
        font_size: f32,
        screen_width: u32,
        y_position: u32,
        screen_buffer: &mut [u8],
        screen_stride: u32,
    ) {
        // Measure the widest digit (0-9 and Bengali ০-৯) to determine cell width
        let digit_cell_width = self.measure_max_digit_width(font_size);
        let colon_cell_width = digit_cell_width / 2; // Colon gets narrower cell

        // Calculate total width needed
        let mut total_width: u32 = 0;
        for ch in text.chars() {
            if ch == ':' || ch == '।' {
                total_width += colon_cell_width;
            } else {
                total_width += digit_cell_width;
            }
        }

        // Calculate starting x position to center the whole time
        let x_start = if total_width < screen_width {
            ((screen_width - total_width) / 2) as i32
        } else {
            0
        };

        // Render each character in its fixed cell
        let mut current_x = x_start;
        for ch in text.chars() {
            let cell_width = if ch == ':' || ch == '।' {
                colon_cell_width
            } else {
                digit_cell_width
            };

            // Render this character
            let char_str = ch.to_string();
            let (char_width, char_height, char_pixels) = self.render_text(&char_str, font_size);

            // Center character within its cell
            let char_x = current_x + ((cell_width as i32 - char_width as i32) / 2);

            // Blit the character to the screen buffer
            for ty in 0..char_height {
                for tx in 0..char_width {
                    let sx = char_x + tx as i32;
                    let sy = y_position as i32 + ty as i32;

                    if sx >= 0 && sx < screen_stride as i32 / 4 && sy >= 0 {
                        let src_idx = ((ty * char_width + tx) * 4) as usize;
                        let dst_idx = ((sy as u32 * screen_stride) + (sx as u32 * 4)) as usize;

                        if dst_idx + 3 < screen_buffer.len() && src_idx + 3 < char_pixels.len() {
                            let alpha = char_pixels[src_idx + 3];
                            if alpha > 0 {
                                screen_buffer[dst_idx] = char_pixels[src_idx];
                                screen_buffer[dst_idx + 1] = char_pixels[src_idx + 1];
                                screen_buffer[dst_idx + 2] = char_pixels[src_idx + 2];
                                screen_buffer[dst_idx + 3] = char_pixels[src_idx + 3];
                            }
                        }
                    }
                }
            }

            current_x += cell_width as i32;
        }
    }

    /// Measure the maximum width of any digit (0-9 and Bengali ০-৯)
    /// Uses caching to avoid re-measuring every frame
    fn measure_max_digit_width(&mut self, font_size: f32) -> u32 {
        // Use font_size bits as cache key (avoids f32 hashing issues)
        let cache_key = font_size.to_bits();

        if let Some(&cached_width) = self.digit_width_cache.get(&cache_key) {
            return cached_width;
        }

        let digits = [
            '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '০', '১', '২', '৩', '৪', '৫', '৬',
            '৭', '৮', '৯',
        ];

        let mut max_width: u32 = 0;
        for digit in digits {
            let (width, _, _) = self.render_text(&digit.to_string(), font_size);
            max_width = max_width.max(width);
        }

        // Add small padding to prevent any overlap
        let result = max_width + 2;
        self.digit_width_cache.insert(cache_key, result);
        result
    }
}
