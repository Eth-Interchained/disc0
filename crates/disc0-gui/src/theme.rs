//! The disc0 visual language.
//!
//! One rule governs colour here: **the accent on a finding IS its consequence
//! class.** Colour is never decoration in this app — it is the fastest channel
//! for "which of these should I be careful about", so it carries meaning or it
//! isn't used.

use forge_ui::{Color, Theme};

pub const FIELD: Color = Color(0x0B1F1D); // deep ground; panels sit on top of it
pub const PANEL: Color = Color(0x0E3B37); // forest
pub const RAISED: Color = Color(0x14524C);
pub const CREAM: Color = Color(0xF7F5F2);
pub const BRASS: Color = Color(0xB89968);
pub const SAGE: Color = Color(0xA7B1A3);
pub const COPPER: Color = Color(0xA8644A);
pub const TEAL: Color = Color(0x167A6E);

pub fn theme() -> Theme {
    Theme {
        background: FIELD,
        panel: PANEL,
        elevated: RAISED,
        border: BRASS.mix(FIELD, 0.6),
        text: CREAM,
        muted: SAGE,
        accent: BRASS,
        on_accent: FIELD,
        success: TEAL,
    }
}

/// Consequence class → accent. The whole colour system in one function.
///
/// sage   = rebuildable, cheap to lose
/// brass  = restorable but it costs you a download, auth, install scripts
/// copper = potentially unique, or we do not know — look here first
pub fn accent_for(consequence: &str) -> Color {
    match consequence {
        "conditional_rebuild" => SAGE,
        "requires_download" => BRASS,
        _ => COPPER,
    }
}

/// Evidence strength → colour. Weak evidence must never look confident.
pub fn evidence_color(evidence: &str) -> Color {
    match evidence {
        "corroborated" => SAGE,
        "partial" => BRASS,
        _ => COPPER,
    }
}

/// Letter-spaced small caps. The painter has no tracking control, so the
/// spacing is inserted into the string.
pub fn tracked(s: &str) -> String {
    s.to_uppercase()
        .chars()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Thousands separators, so 44586 reads as 44,586.
pub fn commas(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
