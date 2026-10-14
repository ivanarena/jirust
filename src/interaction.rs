//! Semantic mouse targets; values are identities, never guessed text or colours.
use crate::data::Ticket;
use ratatui::layout::Rect;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpicFilter {
    pub key: String,
    pub label: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilterTarget {
    Assignee { value: String, label: String },
    Status(String),
    Epic(EpicFilter),
}
impl FilterTarget {
    pub fn assignee(t: &Ticket) -> Option<Self> {
        let u = t.user("assignee");
        let value = if t.fields["assignee"].is_null() {
            "unassigned".into()
        } else {
            [
                ("@id:", &u.id),
                ("@email:", &u.email),
                ("@login:", &u.login),
                ("@name:", &u.name),
            ]
            .into_iter()
            .find(|(_, s)| !s.is_empty())
            .map(|(tag, s)| format!("{tag}{s}"))?
        };
        Some(Self::Assignee {
            value,
            label: if u.display_label().is_empty() {
                "Unassigned".into()
            } else {
                u.display_label()
            },
        })
    }
    pub fn epic(t: &Ticket) -> Option<Self> {
        t.parent_epic().map(|p| {
            Self::Epic(EpicFilter {
                key: p.key.to_uppercase(),
                label: p.name,
            })
        })
    }
    pub fn own_epic(t: &Ticket) -> Option<Self> {
        t.is_epic().then(|| {
            Self::Epic(EpicFilter {
                key: t.key.to_uppercase(),
                label: t.summary(),
            })
        })
    }
}
#[derive(Clone)]
pub struct FilterHit {
    pub rect: Rect,
    pub source: String,
    pub target: FilterTarget,
    pub detail: bool,
}
#[derive(Clone)]
pub struct Click {
    pub at: std::time::Instant,
    pub view: usize,
    pub position: (u16, u16),
    pub hit: FilterHit,
}
impl Click {
    pub fn follows(&self, hit: &FilterHit, view: usize, x: u16, y: u16) -> bool {
        self.at.elapsed() <= std::time::Duration::from_millis(350)
            && self.view == view
            && self.hit.source == hit.source
            && self.hit.detail == hit.detail
            && self.hit.target == hit.target
            && self.position.0.abs_diff(x) <= 1
            && self.position.1 == y
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn double_click_requires_same_field_ticket_view_and_position_within_the_window() {
        let hit = FilterHit {
            rect: Rect::new(3, 4, 10, 1),
            source: "DEV-101".into(),
            target: FilterTarget::Status("Done".into()),
            detail: false,
        };
        let mut click = Click {
            at: std::time::Instant::now(),
            view: 2,
            position: (3, 4),
            hit: hit.clone(),
        };
        assert!(click.follows(&hit, 2, 4, 4));
        assert!(!click.follows(&hit, 1, 3, 4));
        assert!(!click.follows(&hit, 2, 5, 4));
        assert!(!click.follows(&hit, 2, 3, 5));
        let mut other = hit.clone();
        other.source = "DEV-102".into();
        assert!(!click.follows(&other, 2, 3, 4));
        other = hit.clone();
        other.detail = true;
        assert!(!click.follows(&other, 2, 3, 4));
        click.at -= std::time::Duration::from_millis(351);
        assert!(!click.follows(&hit, 2, 3, 4));
    }
}
