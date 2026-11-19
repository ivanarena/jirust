//! Visual issue-type icons and status/parent-epic badges; never determine membership or order.
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
pub const EPIC_COLORS_CACHE: &str = "epic-colors-v1";
const EPIC_SLOTS: u32 = 4112;
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct EpicColors {
    slots: BTreeMap<String, u32>,
}
fn epic_background(slot: u32) -> (u8, u8, u8) {
    if slot < 16 {
        return PALETTE[slot as usize];
    }
    let n = ((slot - 16) * 1597) % 4096;
    (
        24 + 14 * (n % 16) as u8,
        24 + 14 * ((n / 16) % 16) as u8,
        24 + 14 * ((n / 256) % 16) as u8,
    )
}
impl EpicColors {
    pub fn valid(&self) -> bool {
        self.slots.len() <= EPIC_SLOTS as usize
            && self.slots.iter().all(|(k, s)| {
                k.len() <= 1024
                    && crate::data::valid_key(k)
                    && *k == k.to_uppercase()
                    && *s < EPIC_SLOTS
            })
            && self
                .slots
                .values()
                .map(|s| epic_background(*s))
                .collect::<BTreeSet<_>>()
                .len()
                == self.slots.len()
    }
    pub fn register<'a>(&mut self, tickets: impl IntoIterator<Item = &'a Ticket>) -> bool {
        let keys = tickets
            .into_iter()
            .flat_map(|t| {
                t.parent_epic()
                    .map(|e| e.key)
                    .into_iter()
                    .chain(t.is_epic().then(|| t.key.clone()))
            })
            .map(|k| k.to_uppercase())
            .filter(|k| k.len() <= 1024 && crate::data::valid_key(k))
            .collect::<BTreeSet<_>>();
        let keys = keys
            .into_iter()
            .filter(|k| !self.slots.contains_key(k))
            .collect::<Vec<_>>();
        if keys.is_empty() {
            return false;
        }
        let mut used = self.slots.values().copied().collect::<BTreeSet<_>>();
        let mut used_labs = used
            .iter()
            .map(|s| lab(epic_background(*s)))
            .collect::<Vec<_>>();
        // A spread-out candidate grid for larger epic sets, not a repeating
        // 8/16-color hash. Calculate Lab once, outside the assignment loop.
        let candidates = (0..528)
            .map(|s| (s, lab(epic_background(s))))
            .collect::<Vec<_>>();
        let mut changed = false;
        for key in keys {
            let slot = candidates
                .iter()
                .filter(|(s, _)| !used.contains(s))
                .max_by(|(_, a), (_, b)| {
                    let score = |l: &[f64; 3]| {
                        used_labs
                            .iter()
                            .map(|u| l.iter().zip(u).map(|(a, b)| (a - b).powi(2)).sum::<f64>())
                            .fold(f64::INFINITY, f64::min)
                    };
                    score(a).total_cmp(&score(b))
                })
                .map(|(s, _)| *s)
                .or_else(|| (528..EPIC_SLOTS).find(|s| !used.contains(s)));
            let Some(slot) = slot else {
                break;
            };
            used.insert(slot);
            used_labs.push(lab(epic_background(slot)));
            self.slots.insert(key, slot);
            changed = true;
        }
        changed
    }
    pub fn style_key(&self, key: &str) -> Style {
        self.slots
            .get(&key.to_uppercase())
            .map(|s| style_rgb(epic_background(*s)))
            .unwrap_or_else(|| epic_label_style(key))
    }
    pub fn style(&self, ticket: &Ticket) -> Style {
        ticket
            .parent_epic()
            .map(|e| self.style_key(&e.key))
            .unwrap_or_default()
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
    style_rgb(background(slot))
}
fn style_rgb(rgb: (u8, u8, u8)) -> Style {
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
/// Standalone callers have a key-based fallback; App uses the collision-free
/// account-cached registry for both list and detail labels.
pub fn epic_label_style(key: &str) -> Style {
    let hash = key.to_uppercase().bytes().fold(2166136261u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(16777619)
    });
    style_rgb(epic_background(16 + hash % 4096))
}
/// Single-cell Unicode symbols, without emoji or a Nerd Font dependency.
/// Custom names get a neutral fallback; never infer type from summary/status.
pub fn issue_type_icon(ticket: &Ticket) -> (&'static str, Color) {
    if ticket.is_epic() {
        return ("ϟ", Color::Magenta);
    }
    if ticket.fields["issuetype"]["subtask"].as_bool() == Some(true) {
        return ("⊟", Color::Cyan);
    }
    match ticket.kind().trim().to_lowercase().as_str() {
        "bug" => ("●", Color::Red),
        "task" => ("□", Color::Blue),
        "story" => ("≡", Color::Green),
        "design" => ("✎", Color::Magenta),
        "sub-task" | "subtask" | "sub task" => ("⊟", Color::Cyan),
        "improvement" => ("↗", Color::Cyan),
        "feature" | "new feature" => ("◆", Color::Green),
        "spike" => ("◇", Color::Yellow),
        _ => ("◈", Color::Gray),
    }
}
pub fn ticket_key(ticket: &Ticket) -> ratatui::text::Line<'static> {
    use ratatui::text::{Line, Span};
    let (icon, color) = issue_type_icon(ticket);
    Line::from(vec![
        Span::styled(format!("{icon} "), Style::default().fg(color)),
        Span::raw(ticket.key.clone()),
    ])
}
pub fn ticket_summary(ticket: &Ticket, width: u16) -> ratatui::text::Line<'static> {
    let style = ticket
        .parent_epic()
        .map(|e| epic_label_style(&e.key))
        .unwrap_or_default();
    ticket_summary_with_style(ticket, width, style)
}
pub fn ticket_summary_with_style(
    ticket: &Ticket,
    width: u16,
    epic_style: Style,
) -> ratatui::text::Line<'static> {
    use ratatui::text::{Line, Span};
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
    let mut spans = vec![];
    if let Some(epic) = ticket.parent_epic() {
        // Keep title space even when an epic has a long or wide Unicode name.
        let budget = (width / 3).clamp(6, 28).min(width.saturating_sub(8));
        if budget >= 3 {
            let limit = (budget - 2) as usize;
            let mut label = String::new();
            if epic.name.width() <= limit {
                label = epic.name;
            } else {
                let mut cells = 0;
                for c in epic.name.chars() {
                    let n = c.width().unwrap_or(0);
                    if cells + n > limit.saturating_sub(1) {
                        break;
                    }
                    label.push(c);
                    cells += n;
                }
                label.push('…');
            }
            spans.push(Span::styled(format!(" {label} "), epic_style));
            spans.push(Span::raw(" "));
        }
    }
    spans.push(Span::raw(summary(ticket)));
    Line::from(spans)
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
    fn epic_colors_are_unique_contrast_safe_key_based_and_stable_after_restart_and_additions() {
        let mut items = (1..=100)
            .map(|i| {
                let mut t = crate::data::demo_tickets().remove(0);
                t.key = format!("DEV-{i}");
                t.fields["issuetype"] = json!({"name":"Epic"});
                t
            })
            .collect::<Vec<_>>();
        let mut colors = EpicColors::default();
        assert!(colors.register(&items));
        assert!(colors.valid());
        assert_eq!(colors.slots.len(), 100);
        assert_eq!(
            items
                .iter()
                .map(|t| epic_background(colors.slots[&t.key]))
                .collect::<BTreeSet<_>>()
                .len(),
            100
        );
        for t in &items {
            let s = colors.style_key(&t.key);
            let Color::Rgb(r, g, b) = s.bg.unwrap() else {
                panic!()
            };
            let l = luminance((r, g, b));
            let contrast = if s.fg == Some(Color::Rgb(0, 0, 0)) {
                (l + 0.05) / 0.05
            } else {
                1.05 / (l + 0.05)
            };
            assert!(contrast >= 4.5);
        }
        let before = colors.slots.clone();
        let mut restored: EpicColors =
            serde_json::from_slice(&serde_json::to_vec(&colors).unwrap()).unwrap();
        assert!(restored.valid());
        items[0].fields["summary"] = json!("Renamed epic");
        assert!(!restored.register(&items));
        assert_eq!(restored.style_key("dev-1"), colors.style_key("DEV-1"));
        let mut new = items[0].clone();
        new.key = "DEV-200".into();
        assert!(restored.register([&new]));
        for (k, s) in before {
            assert_eq!(restored.slots[&k], s);
        }
        let invalid: EpicColors =
            serde_json::from_value(json!({"slots":{"DEV-1":0,"DEV-2":0}})).unwrap();
        assert!(!invalid.valid());
        let invalid: EpicColors = serde_json::from_value(json!({"slots":{"DEV-1":4112}})).unwrap();
        assert!(!invalid.valid());
    }
    #[test]
    fn only_parent_labels_are_colored_truncated_by_cells_and_precede_plain_ticket_titles() {
        use unicode_width::UnicodeWidthStr;
        let mut t = crate::data::demo_tickets().remove(0);
        let plain = ticket_summary(&t, 30);
        assert_eq!(plain.spans.len(), 1);
        assert!(plain.spans[0].style.bg.is_none());
        t.fields["parent"] = json!({"key":"DEV-900","fields":{"summary":"界界界界界界界界界界界界界界界界界","issuetype":{"name":"Epic"}}});
        let line = ticket_summary(&t, 30);
        assert_eq!(line.spans.len(), 3);
        assert_eq!(line.spans[0].style, epic_label_style("DEV-900"));
        assert!(line.spans[0].content.width() <= 10);
        assert_eq!(line.spans[2].content, t.summary());
        assert!(line.spans[2].style.bg.is_none());
        t.fields["issuetype"] = json!({"name":"Epic"});
        let line = ticket_summary(&t, 30);
        assert_eq!(line.spans.len(), 1);
        assert!(line.spans[0].style.bg.is_none());
    }
    #[test]
    fn type_icons_are_single_cell_explicit_and_have_a_custom_fallback() {
        use unicode_width::UnicodeWidthStr;
        let mut t = ticket("Done");
        let mut icons = BTreeSet::new();
        for kind in [
            "Bug",
            "Task",
            "Story",
            "Design",
            "Epic",
            "Sub-task",
            "Improvement",
            "New Feature",
            "Spike",
            "Custom type",
            "",
        ] {
            t.fields["issuetype"] = json!({"name":kind});
            let before = t.fields.clone();
            let (icon, _) = issue_type_icon(&t);
            assert_eq!(icon.width(), 1, "{kind}");
            assert_eq!(ticket_key(&t).width(), t.key.width() + 2);
            assert_eq!(t.fields, before);
            if !kind.is_empty() {
                assert!(icons.insert(icon), "{kind}");
            }
        }
        t.fields["issuetype"] = json!({"name":"  bUg  "});
        assert_eq!(issue_type_icon(&t).0, "●");
        t.fields["issuetype"] = json!({"name":"Localized child", "subtask":true});
        assert_eq!(issue_type_icon(&t).0, "⊟");
        t.fields["issuetype"] = json!({"name":"Localized epic", "hierarchyLevel":1});
        assert_eq!(issue_type_icon(&t).0, "ϟ");
        t.fields["issuetype"] = serde_json::Value::Null;
        t.fields["summary"] = json!("Bug in a design task");
        assert_eq!(issue_type_icon(&t).0, "◈");
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
