//! Visual status badges only; colors never determine Jira membership or order.
use crate::data::Ticket;
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const STATUS_COLORS_CACHE: &str = "status-colors-v1";
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct StatusColors {
    slots: BTreeMap<String, u32>,
}
impl StatusColors {
    pub fn valid(&self) -> bool {
        self.slots.len() <= 100000
            && self
                .slots
                .keys()
                .all(|n| !n.is_empty() && n.len() <= 1024 && *n == n.to_lowercase())
            && self.slots.values().all(|s| *s < 512016)
            && self
                .slots
                .values()
                .map(|s| foreground(*s))
                .collect::<BTreeSet<_>>()
                .len()
                == self.slots.len()
    }
    /// Append colors for newly observed names without changing existing colors.
    pub fn register<'a>(&mut self, tickets: impl IntoIterator<Item = &'a Ticket>) -> bool {
        let names: BTreeSet<_> = tickets
            .into_iter()
            .map(|t| t.status().to_lowercase())
            .filter(|n| !n.is_empty())
            .collect();
        let mut changed = false;
        let mut used: BTreeSet<_> = self.slots.values().map(|s| foreground(*s)).collect();
        let mut slot = self.slots.values().max().map(|s| s + 1).unwrap_or(0);
        for name in names {
            if self.slots.contains_key(&name) {
                continue;
            }
            while slot < 512016 && used.contains(&foreground(slot)) {
                slot += 1;
            }
            if slot == 512016 {
                break;
            }
            used.insert(foreground(slot));
            self.slots.insert(name, slot);
            changed = true;
        }
        changed
    }
    pub fn style(&self, ticket: &Ticket) -> Style {
        self.style_name(&ticket.status())
    }
    pub fn style_name(&self, name: &str) -> Style {
        self.slots
            .get(&name.to_lowercase())
            .map(|s| style_for(*s))
            .unwrap_or_else(|| fallback_style(name))
    }
}
fn foreground(slot: u32) -> (u8, u8, u8) {
    const PALETTE: [(u8, u8, u8); 16] = [
        (35, 80, 155),
        (25, 105, 60),
        (115, 50, 155),
        (155, 65, 25),
        (20, 105, 115),
        (145, 40, 90),
        (90, 85, 25),
        (160, 35, 35),
        (65, 75, 90),
        (55, 45, 135),
        (105, 70, 40),
        (0, 90, 145),
        (85, 105, 35),
        (145, 45, 135),
        (130, 90, 10),
        (20, 95, 85),
    ];
    if let Some(rgb) = PALETTE.get(slot as usize) {
        return *rgb;
    }
    // Injective permutation of 80³ dark RGB colors beyond the main palette;
    // registration skips collisions with those initial hand-picked colors.
    let n = ((u64::from(slot - 16) * 15485863) % 512000) as u32;
    (
        15 + (n % 80) as u8,
        15 + ((n / 80) % 80) as u8,
        15 + ((n / 6400) % 80) as u8,
    )
}
fn style_for(slot: u32) -> Style {
    let (r, g, b) = foreground(slot);
    Style::default()
        .fg(Color::Rgb(r, g, b))
        .bg(Color::Rgb(220 + r / 8, 220 + g / 8, 220 + b / 8))
        .add_modifier(Modifier::BOLD)
}
fn fallback_style(name: &str) -> Style {
    let hash = name.to_lowercase().bytes().fold(2166136261u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(16777619)
    });
    style_for(16 + hash % 512000)
}
/// Standalone document callers use a deterministic fallback; App uses its
/// account-cached collision-free palette consistently in every view/document.
pub fn status_style(ticket: &Ticket) -> Style {
    fallback_style(&ticket.status())
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
    #[test]
    fn every_status_has_a_distinct_color_even_within_the_same_category() {
        let items = (0..64)
            .map(|i| {
                let mut t = crate::data::demo_tickets().remove(0);
                t.fields["status"] =
                    json!({"name":format!("Status {i}"),"statusCategory":{"key":"done"}});
                t
            })
            .collect::<Vec<_>>();
        let mut colors = StatusColors::default();
        assert!(colors.register(&items));
        assert!(colors.valid());
        let styles = items.iter().map(|t| colors.style(t)).collect::<Vec<_>>();
        for (i, s) in styles.iter().enumerate() {
            assert!(styles[..i].iter().all(|v| v.fg != s.fg));
        }
        let restored: StatusColors =
            serde_json::from_slice(&serde_json::to_vec(&colors).unwrap()).unwrap();
        for t in &items {
            assert_eq!(colors.style(t), restored.style(t));
        }
        let before = colors.style(&items[0]);
        let mut extra = items[0].clone();
        extra.fields["status"] = json!({"name":"A newly added status"});
        colors.register([&extra]);
        assert_eq!(before, colors.style(&items[0]));
        assert!(!colors.register(&items));
        assert_ne!(colors.style(&items[0]), colors.style(&items[1]));
    }
}
