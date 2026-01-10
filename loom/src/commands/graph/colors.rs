//! Stage color assignment for visual differentiation
//!
//! Provides deterministic color assignment based on stage ID hash to make
//! dependencies visually clearer across different views.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use colored::Color;

/// Available terminal colors that work well on both dark and light backgrounds
const STAGE_COLORS: [Color; 16] = [
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::BrightRed,
    Color::BrightGreen,
    Color::BrightYellow,
    Color::BrightBlue,
    Color::BrightMagenta,
    Color::BrightCyan,
    Color::TrueColor { r: 255, g: 165, b: 0 }, // Orange
    Color::TrueColor { r: 128, g: 0, b: 128 },   // Purple
    Color::TrueColor { r: 0, g: 128, b: 128 },   // Teal
    Color::TrueColor { r: 255, g: 192, b: 203 }, // Pink
];

/// Deterministically assign a color to a stage based on its ID
///
/// Uses a hash of the stage ID to select from a palette of 16 terminal-appropriate
/// colors. The same stage ID will always get the same color.
pub fn stage_color(stage_id: &str) -> Color {
    let mut hasher = DefaultHasher::new();
    stage_id.hash(&mut hasher);
    let hash = hasher.finish();
    let index = (hash % STAGE_COLORS.len() as u64) as usize;
    STAGE_COLORS[index]
}

/// Get a color by index, cycling through the palette
///
/// Used for position-based color assignment where adjacent items
/// should have different colors.
pub fn color_by_index(index: usize) -> Color {
    STAGE_COLORS[index % STAGE_COLORS.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_color_assignment() {
        // Same ID should always get same color
        let id = "test-stage";
        let color1 = stage_color(id);
        let color2 = stage_color(id);
        assert_eq!(format!("{:?}", color1), format!("{:?}", color2));
    }

    #[test]
    fn test_different_ids_may_have_different_colors() {
        // Different IDs should potentially get different colors
        let color1 = stage_color("stage-one");
        let color2 = stage_color("stage-two");

        // Note: We can't guarantee they're different due to hash collisions,
        // but we can verify they're both valid colors
        let valid_colors: Vec<String> = STAGE_COLORS
            .iter()
            .map(|c| format!("{:?}", c))
            .collect();

        assert!(valid_colors.contains(&format!("{:?}", color1)));
        assert!(valid_colors.contains(&format!("{:?}", color2)));
    }

    #[test]
    fn test_color_palette_size() {
        assert_eq!(STAGE_COLORS.len(), 16);
    }
}
