//! Visual status badges only; colors never determine Jira membership or order.
use crate::data::Ticket;
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

// Intentionally replace the older pale palette, keeping the new assignments stable.
pub const STATUS_COLORS_CACHE: &str = "status-colors-v2";
const MAX_SLOTS: u32 = 512016;
const PALETTE: [(u8, u8, u8); 16] = [
    (16, 125, 58),   // Done: green, reserved even before it is observed.
    (190, 30, 40),   // Discarded: red, never allocated to another name.
    (25, 85, 190),   // Blue
    (112, 45, 166),  // Violet
    (242, 193, 43),  // Gold
    (34, 196, 218),  // Cyan
    (211, 42, 142),  // Magenta
    (185, 209, 49),  // Lime
    (74, 88, 112),   // Slate
    (242, 119, 28),  // Orange
    (53, 48, 133),   // Indigo
    (9, 139, 139),   // Teal
    (123, 72, 31),   // Brown
    (181, 153, 238), // Lavender
    (243, 144, 169), // Rose
    (133, 195, 247), // Light blue
];
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct StatusColors {
    slots: BTreeMap<String, u32>,
}
fn semantic_slot(name: &str) -> Option<u32> {
    match name {
        "done" => Some(0),
        "discarded" => Some(1),
        _ => None,
    }
}
impl StatusColors {
    pub fn valid(&self) -> bool {
        self.slots.len() <= 100000
            && self.slots.iter().all(|(n, s)| {
                !n.is_empty()
                    && n.len() <= 1024
                    && *n == n.to_lowercase()
                    && *s < MAX_SLOTS
                    && match semantic_slot(n) {
                        Some(reserved) => *s == reserved,
                        None => *s >= 2,
                    }
            })
            && self
                .slots
                .values()
                .map(|s| background(*s))
                .collect::<BTreeSet<_>>()
                .len()
                == self.slots.len()
    }
    /// Maximize perceptual separation when adding colors; never recolor existing names.
    pub fn register<'a>(&mut self, tickets: impl IntoIterator<Item = &'a Ticket>) -> bool {
        let names = tickets
            .into_iter()
            .map(|t| t.status().to_lowercase())
            .filter(|n| !n.is_empty() && n.len() <= 1024)
            .collect::<BTreeSet<_>>();
        let mut changed = false;
        let mut used = self
            .slots
            .values()
            .map(|s| background(*s))
            .chain([PALETTE[0], PALETTE[1]])
            .collect::<BTreeSet<_>>();
        let mut next = self
            .slots
            .values()
            .max()
            .map(|s| s + 1)
            .unwrap_or(16)
            .max(16);
        for name in names {
            if self.slots.contains_key(&name) {
                continue;
            }
            if self.slots.len() == 100000 {
                break;
            }
            let slot = if let Some(s) = semantic_slot(&name) {
                s
            } else if let Some(s) =
                (2..16)
                    .filter(|s| !used.contains(&background(*s)))
                    .max_by(|a, b| {
                        separation(background(*a), &used)
                            .total_cmp(&separation(background(*b), &used))
                    })
            {
                s
            } else {
                while next < MAX_SLOTS && used.contains(&background(next)) {
                    next += 1;
                }
                if next == MAX_SLOTS {
                    break;
                }
                let s = next;
                next += 1;
                s
            };
            used.insert(background(slot));
            self.slots.insert(name, slot);
            changed = true;
        }
        changed
    }
    pub fn style(&self, ticket: &Ticket) -> Style {
        self.style_name(&ticket.status())
    }
    pub fn style_name(&self, name: &str) -> Style {
        let normalized = name.to_lowercase();
        semantic_slot(&normalized)
            .or_else(|| self.slots.get(&normalized).copied())
            .map(style_for)
            .unwrap_or_else(|| fallback_style(name))
    }
}
fn background(slot: u32) -> (u8, u8, u8) {
    if let Some(rgb) = PALETTE.get(slot as usize) {
        return *rgb;
    }
    // Injective permutation for unusually large workflows; skip palette collisions.
    let n = ((u64::from(slot - 16) * 15485863) % 512000) as u32;
    (
        70 + (n % 80) as u8,
        70 + ((n / 80) % 80) as u8,
        70 + ((n / 6400) % 80) as u8,
    )
}
fn linear(c: u8) -> f64 {
    let c = f64::from(c) / 255.;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
fn luminance((r, g, b): (u8, u8, u8)) -> f64 {
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}
fn lab((r, g, b): (u8, u8, u8)) -> [f64; 3] {
    let (r, g, b) = (linear(r), linear(g), linear(b));
    let f = |v: f64| {
        if v > 216. / 24389. {
            v.cbrt()
        } else {
            v * (24389. / 27.) / 116. + 16. / 116.
        }
    };
    let x = f((0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047);
    let y = f(0.2126729 * r + 0.7151522 * g + 0.0721750 * b);
    let z = f((0.0193339 * r + 0.1191920 * g + 0.9503041 * b) / 1.08883);
    [116. * y - 16., 500. * (x - y), 200. * (y - z)]
}
fn distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    lab(a)
        .into_iter()
        .zip(lab(b))
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt()
}
fn separation(rgb: (u8, u8, u8), used: &BTreeSet<(u8, u8, u8)>) -> f64 {
    used.iter()
        .map(|c| distance(rgb, *c))
        .fold(f64::INFINITY, f64::min)
}
fn style_for(slot: u32) -> Style {
    let rgb = background(slot);
    let l = luminance(rgb);
    // Choose whichever has the greater WCAG contrast against this solid badge.
    let text = if (l + 0.05) / 0.05 >= 1.05 / (l + 0.05) {
        Color::Rgb(0, 0, 0)
    } else {
        Color::Rgb(255, 255, 255)
    };
    Style::default()
        .fg(text)
        .bg(Color::Rgb(rgb.0, rgb.1, rgb.2))
        .add_modifier(Modifier::BOLD)
}
fn fallback_style(name: &str) -> Style {
    if let Some(s) = semantic_slot(&name.to_lowercase()) {
        return style_for(s);
    }
    let hash = name.to_lowercase().bytes().fold(2166136261u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(16777619)
    });
    style_for(16 + hash % 512000)
}
/// Standalone document callers use a deterministic fallback; App uses its
/// account-cached palette consistently in every view/document.
pub fn status_style(ticket: &Ticket) -> Style {
    fallback_style(&ticket.status())
}
/// Compact visual labels only. Raw workflow names remain in filters/actions.
pub fn status_label(name: &str) -> String {
    if name.eq_ignore_ascii_case("Done") {
        "✓".into()
    } else if name.eq_ignore_ascii_case("Discarded") {
        "✕".into()
    } else {
        name.into()
    }
}
pub fn epic_style(selected: bool) -> Style {
    Style::default()
        .fg(Color::Rgb(88, 28, 140))
        .bg(if selected {
            Color::Rgb(216, 187, 246)
        } else {
            Color::Rgb(241, 229, 255)
        })
        .add_modifier(Modifier::BOLD)
}
pub fn summary(ticket: &Ticket) -> String {
    if ticket.is_epic() {
        format!("Epic · {}", ticket.summary())
    } else {
        ticket.summary()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn ticket(name: &str) -> Ticket {
        let mut t = crate::data::demo_tickets().remove(0);
        t.fields["status"] = json!({"name":name,"statusCategory":{"key":"done"}});
        t
    }
    #[test]
    fn completion_icons_are_visual_only_not_status_category_guesses() {
        assert_eq!(status_label("Done"), "✓");
        assert_eq!(status_label("discarded"), "✕");
        assert_eq!(status_label("Finished"), "Finished");
        let t = ticket("Finished");
        let before = t.fields.clone();
        status_label(&t.status());
        assert_eq!(t.fields, before);
    }
    #[test]
    fn semantic_colors_are_reserved_even_if_done_and_discarded_arrive_later() {
        let items = (0..32)
            .map(|i| ticket(&format!("Status {i}")))
            .collect::<Vec<_>>();
        let mut colors = StatusColors::default();
        colors.register(&items);
        let done = colors.style_name("DONE");
        let discarded = colors.style_name("Discarded");
        assert_eq!(done.bg, Some(Color::Rgb(16, 125, 58)));
        assert_eq!(discarded.bg, Some(Color::Rgb(190, 30, 40)));
        assert!(
            items
                .iter()
                .all(|t| colors.style(t).bg != done.bg && colors.style(t).bg != discarded.bg)
        );
        assert_eq!(status_style(&ticket("Done")), done);
        assert_eq!(status_style(&ticket("Discarded")), discarded);
        colors.register([&ticket("Done"), &ticket("Discarded")]);
        assert!(colors.valid());
        assert_eq!(done, colors.style_name("Done"));
        let mut wrong = colors.clone();
        wrong.slots.insert("done".into(), 5);
        assert!(!wrong.valid());
    }
    #[test]
    fn ten_statuses_have_widely_separated_backgrounds_and_readable_text() {
        let items = [
            "Done",
            "Discarded",
            "Queued",
            "In Progress",
            "Review",
            "Open",
            "Blocked",
            "Refinement",
            "Ready",
            "To Do",
        ]
        .map(ticket);
        let mut colors = StatusColors::default();
        colors.register(&items);
        let rgb = items
            .iter()
            .map(|t| background(colors.slots[&t.status().to_lowercase()]))
            .collect::<Vec<_>>();
        for (i, c) in rgb.iter().enumerate() {
            for other in &rgb[..i] {
                assert!(
                    distance(*c, *other) >= 30.,
                    "Badges too similar: {c:?} and {other:?}"
                );
            }
        }
        for s in 0..MAX_SLOTS {
            let bg = luminance(background(s));
            let fg = if style_for(s).fg == Some(Color::Rgb(0, 0, 0)) {
                0.
            } else {
                1.
            };
            let contrast = (bg.max(fg) + 0.05) / (bg.min(fg) + 0.05);
            assert!(contrast >= 4.5);
        }
    }
    #[test]
    fn every_status_has_a_distinct_color_even_within_the_same_category() {
        let items = (0..64)
            .map(|i| ticket(&format!("Status {i}")))
            .collect::<Vec<_>>();
        let mut colors = StatusColors::default();
        assert!(colors.register(&items));
        assert!(colors.valid());
        let styles = items.iter().map(|t| colors.style(t)).collect::<Vec<_>>();
        for (i, s) in styles.iter().enumerate() {
            assert!(styles[..i].iter().all(|v| v.bg != s.bg));
        }
        let restored: StatusColors =
            serde_json::from_slice(&serde_json::to_vec(&colors).unwrap()).unwrap();
        for t in &items {
            assert_eq!(colors.style(t), restored.style(t));
        }
        let before = colors.style(&items[0]);
        colors.register([&ticket("A newly added status")]);
        assert_eq!(before, colors.style(&items[0]));
        assert!(!colors.register(&items));
        assert_ne!(colors.style(&items[0]), colors.style(&items[1]));
    }
}
