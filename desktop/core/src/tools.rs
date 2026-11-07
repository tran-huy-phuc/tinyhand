//! Pen thickness.

/// Pen width multipliers, smallest first. Index 1 is the original width.
pub const PEN_SIZES: [f32; 4] = [0.6, 1.0, 1.7, 2.6];
pub const DEFAULT_PEN_SIZE: u8 = 1;

pub fn pen_scale(size: u8) -> f32 {
    PEN_SIZES[(size as usize).min(PEN_SIZES.len() - 1)]
}
