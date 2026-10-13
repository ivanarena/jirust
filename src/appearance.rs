//! Visual status badges only; colors never determine Jira membership or order.
use crate::data::Ticket;
use ratatui::style::{Color, Modifier, Style};

pub fn status_style(ticket: &Ticket) -> Style {
    let name = ticket.status().to_lowercase();
    let hash = name.bytes().fold(2166136261u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(16777619)
    });
    let (fg, bg) = match ticket.fields["status"]["statusCategory"]["key"].as_str() {
        Some("done") => ((25, 105, 60), (220, 244, 227)),
        Some("new") => ((35, 80, 155), (224, 236, 255)),
        _ => [
            ((115, 50, 155), (240, 228, 252)),
            ((155, 65, 25), (255, 234, 217)),
            ((20, 105, 115), (215, 244, 246)),
            ((145, 40, 90), (252, 224, 238)),
            ((90, 85, 25), (245, 241, 211)),
            ((45, 65, 145), (228, 233, 255)),
            ((125, 55, 55), (250, 228, 228)),
            ((55, 105, 70), (226, 244, 232)),
        ][hash as usize % 8],
    };
    Style::default()
        .fg(Color::Rgb(fg.0, fg.1, fg.2))
        .bg(Color::Rgb(bg.0, bg.1, bg.2))
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
    #[test]
    fn status_colors_are_stable_category_aware_and_visual_only() {
        let mut t = crate::data::demo_tickets().remove(0);
        t.fields["status"] = json!({"name":"Done","statusCategory":{"key":"done"}});
        let done = status_style(&t);
        assert_eq!(done, status_style(&t));
        t.fields["status"] = json!({"name":"Queued","statusCategory":{"key":"new"}});
        assert_ne!(done, status_style(&t));
        t.fields["status"] = json!({"name":"Code Review"});
        let review = status_style(&t);
        t.fields["status"] = json!({"name":"In Progress"});
        assert_ne!(review, status_style(&t));
        let before = t.clone();
        status_style(&t);
        assert_eq!(before.fields, t.fields);
    }
}
