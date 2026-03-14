//! Tem mascot ASCII art for the welcome/onboarding screen.
//!
//! Based on Tem's pixel art mascot — a black-hooded cat with
//! heterochromia eyes, a pink heart bandana, and sparkles.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme::TemPalette;

/// Get the Tem mascot ASCII art as styled lines.
pub fn tem_mascot(accent_style: Style, secondary_style: Style) -> Vec<Line<'static>> {
    let pink = Style::default().fg(TemPalette::HOT_PINK);
    let amber = Style::default().fg(TemPalette::AMBER);
    let blue = Style::default().fg(TemPalette::ICE_BLUE);
    let lavender = Style::default().fg(TemPalette::LAVENDER);
    let white = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);

    vec![
        // Sparkle + ears
        Line::from(vec![
            Span::styled("        ", dim),
            Span::styled("+", amber),
            Span::styled("    /\\", white),
            Span::styled("    ", dim),
            Span::styled("/\\", white),
            Span::styled("    ", dim),
            Span::styled("+", amber),
        ]),
        // Ear inners + head top
        Line::from(vec![
            Span::styled("         ", dim),
            Span::styled("  /", white),
            Span::styled("^^", lavender),
            Span::styled("\\__/", white),
            Span::styled("^^", lavender),
            Span::styled("\\", white),
        ]),
        // Eyes line
        Line::from(vec![
            Span::styled("         ", dim),
            Span::styled(" |", white),
            Span::styled(" \u{25a0}", amber),
            Span::styled("  ", white),
            Span::styled("1", amber),
            Span::styled("  ", white),
            Span::styled("\u{25a0} ", blue),
            Span::styled("|", white),
        ]),
        // Mouth
        Line::from(vec![
            Span::styled("         ", dim),
            Span::styled(" |", white),
            Span::styled("   ", dim),
            Span::styled("w", white),
            Span::styled("   |", white),
        ]),
        // Bandana top
        Line::from(vec![
            Span::styled("         ", dim),
            Span::styled("  \\", white),
            Span::styled("~~~~~", pink),
            Span::styled("/", white),
        ]),
        // Bandana heart
        Line::from(vec![
            Span::styled("         ", dim),
            Span::styled("   ", dim),
            Span::styled("\\", pink),
            Span::styled(" \u{2665} ", pink.add_modifier(Modifier::BOLD)),
            Span::styled("/", pink),
        ]),
        // Body + waving paw
        Line::from(vec![
            Span::styled("      ", dim),
            Span::styled("o", white),
            Span::styled("/", dim),
            Span::styled("  ", dim),
            Span::styled("(     )", white),
        ]),
        // Feet
        Line::from(vec![
            Span::styled("         ", dim),
            Span::styled("   ", dim),
            Span::styled("U", lavender),
            Span::styled(" ", dim),
            Span::styled("U", lavender),
        ]),
        // Spacing
        Line::from(""),
        // Title
        Line::from(vec![
            Span::styled("      T E M M", accent_style.add_modifier(Modifier::BOLD)),
            Span::styled("1", amber.add_modifier(Modifier::BOLD)),
            Span::styled("E", accent_style.add_modifier(Modifier::BOLD)),
        ]),
        // Subtitle
        Line::from(Span::styled("    Cloud-native AI Agent", secondary_style)),
    ]
}
