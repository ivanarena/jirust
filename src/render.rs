use crate::{
    app::{App, Dialog, Focus, SearchMode},
    data, markdown,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Text},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, Wrap},
};
use ratatui_image::Image;
use unicode_width::UnicodeWidthStr;

fn block(title: impl Into<Line<'static>>, active: bool) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if active { Color::Blue } else { Color::DarkGray }))
}
fn selection() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Rgb(215, 232, 248))
        .add_modifier(Modifier::BOLD)
}
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width.min(area.width),
        height.min(area.height),
    )
}
pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    if area.width < 60 || area.height < 16 {
        frame.render_widget(
            Paragraph::new("jiraui needs 60×16 cells. Resize/zoom this pane. q quits."),
            area,
        );
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
    let scope = if app.settings.scope_name.is_empty() {
        app.settings.prefix().to_string()
    } else {
        app.settings.scope_name.clone()
    };
    let brand = format!(
        " jiraui {} ",
        if scope.is_empty() {
            "· scope?"
        } else {
            &scope
        }
    );
    let brand_width = (brand.width() as u16).min(area.width / 3).max(10);
    app.scope_rect = Rect::new(rows[0].x, rows[0].y, brand_width, 1);
    frame.render_widget(
        Paragraph::new(brand).style(
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ),
        app.scope_rect,
    );
    app.tabs.clear();
    let names = app.view_names();
    let mut x = app.scope_rect.right() + 1;
    for (index, name) in names.into_iter().enumerate() {
        let label = format!(" ^{} {} ", index + 1, data::line(&name));
        let w = label.width() as u16;
        if x + w > area.right() {
            break;
        };
        let rect = Rect::new(x, rows[0].y, w, 1);
        frame.render_widget(
            Paragraph::new(label).style(if index == app.view {
                selection()
            } else {
                Style::default().fg(Color::DarkGray)
            }),
            rect,
        );
        app.tabs.push((rect, index));
        x += w + 1;
    }
    let controls = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(22),
            Constraint::Length(23),
            Constraint::Length(22),
        ])
        .split(rows[1]);
    app.search_rect = controls[0];
    app.assignee_rect = controls[1];
    app.status_rect = controls[2];
    let search = match app.search_mode {
        SearchMode::Number => format!(" Find {}-{}", app.settings.prefix(), app.search),
        SearchMode::Text => format!(" / {}", app.search),
        SearchMode::None => {
            if app.lookup_active {
                format!(
                    " Find {}-{} · Esc restores list",
                    app.settings.prefix(),
                    app.search
                )
            } else if app.remote_search.is_empty() {
                " / Search · type digits to find a ticket".into()
            } else {
                format!(" / {}", app.remote_search)
            }
        }
    };
    frame.render_widget(
        Paragraph::new(search.clone()).style(Style::default().fg(Color::DarkGray)),
        controls[0],
    );
    frame.render_widget(
        Paragraph::new(format!(
            " Assignee: {} ▾",
            if app.assignee_filter.is_empty() {
                "All"
            } else {
                &app.assignee_filter
            }
        ))
        .style(Style::default().fg(Color::Blue)),
        controls[1],
    );
    frame.render_widget(
        Paragraph::new(format!(
            " Status: {} ▾",
            if app.status_filter.is_empty() {
                "Any"
            } else {
                &app.status_filter
            }
        ))
        .style(Style::default().fg(Color::Blue)),
        controls[2],
    );
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(rows[2]);
    app.tickets_rect = panes[0];
    app.ticket_rect = panes[1];
    let left_block = block(
        format!(
            " Tickets · {}{} ",
            app.visible.len(),
            if app.loading { " · loading" } else { "" }
        ),
        app.focus == Focus::Tickets,
    );
    let inner = left_block.inner(panes[0]);
    app.row_height = if inner.width < 65 { 2 } else { 1 };
    let table = if app.row_height == 1 {
        let header = Row::new(["Key", "Status", "Assignee", "Summary"])
            .style(Style::default().fg(Color::DarkGray));
        let data_rows = app.visible.iter().map(|i| {
            let t = &app.items[*i];
            Row::new(vec![
                Cell::from(t.key.clone()),
                Cell::from(t.status()),
                Cell::from(t.user("assignee").label()),
                Cell::from(t.summary()),
            ])
        });
        Table::new(
            data_rows,
            [
                Constraint::Length(11),
                Constraint::Length(13),
                Constraint::Length(14),
                Constraint::Min(12),
            ],
        )
        .header(header)
    } else {
        let header = Row::new(["Key", "Ticket / Assignee · Status"])
            .style(Style::default().fg(Color::DarkGray));
        let data_rows = app.visible.iter().map(|i| {
            let t = &app.items[*i];
            Row::new(vec![
                Cell::from(t.key.clone()),
                Cell::from(Text::from(vec![
                    Line::from(t.summary()),
                    Line::styled(
                        format!("{} · {}", t.user("assignee").label(), t.status()),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])),
            ])
            .height(2)
        });
        Table::new(data_rows, [Constraint::Length(11), Constraint::Min(20)]).header(header)
    }
    .column_spacing(1)
    .row_highlight_style(selection())
    .block(left_block);
    frame.render_stateful_widget(table, panes[0], &mut app.table);
    let right_block = block(
        if app.detail_key.is_empty() {
            " Ticket ".into()
        } else {
            format!(
                " {}{} ",
                app.detail_key,
                if app.history.is_empty() {
                    ""
                } else {
                    " · Esc back"
                }
            )
        },
        app.focus == Focus::Ticket,
    );
    let right = right_block.inner(panes[1]);
    frame.render_widget(right_block, panes[1]);
    app.document(right.width);
    app.scroll = app
        .scroll
        .min(app.document.len().saturating_sub(right.height as usize));
    if app.document.is_empty() {
        let caption = if app.detail_key.is_empty() {
            "Select a ticket, or type its number."
        } else if app.pending.contains(&app.detail_key) {
            "Loading full ticket…"
        } else {
            "Full ticket not loaded. r retries."
        };
        frame.render_widget(
            Paragraph::new(caption).style(Style::default().fg(Color::DarkGray)),
            right,
        )
    } else {
        let lines = app
            .document
            .iter()
            .skip(app.scroll)
            .take(right.height as usize)
            .map(|l| l.line())
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), right)
    }
    let mode = if app.read_only {
        "READ ONLY · "
    } else if app.backend.kind == crate::backend::Kind::Demo {
        "DEMO · "
    } else {
        ""
    };
    let status = format!(
        " {mode}{}{}",
        if app.writing { "Updating… · " } else { "" },
        app.status
    );
    frame.render_widget(
        Paragraph::new(status).style(Style::default().fg(
            if !app.error.is_empty() && app.status.contains("failed") {
                Color::Red
            } else {
                Color::DarkGray
            },
        )),
        rows[3],
    );
    app.footer_buttons.clear();
    let buttons = [
        ('a', "Assign"),
        ('t', "Status"),
        ('e', "Edit"),
        ('c', "Comment"),
        ('m', "Move"),
        ('l', "Links"),
        ('O', "Jira"),
        ('g', "Scope"),
        ('f', "Filters"),
        ('?', "Help"),
    ];
    let mut bx = rows[4].x;
    for (key, label) in buttons {
        let text = format!(" {key} {label} ");
        let w = text.width() as u16;
        if bx + w > area.right() {
            break;
        }
        let rect = Rect::new(bx, rows[4].y, w, 1);
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(Color::Blue)),
            rect,
        );
        app.footer_buttons.push((rect, key));
        bx += w;
    }
    if app.search_mode != SearchMode::None && app.dialog.is_none() {
        frame.set_cursor_position((
            controls[0].x + (search.width() as u16).min(controls[0].width.saturating_sub(1)),
            controls[0].y,
        ));
    }
    if let Some((image, name)) = &app.preview {
        let rect = centered(
            area,
            area.width.saturating_sub(8),
            area.height.saturating_sub(6),
        );
        frame.render_widget(Clear, rect);
        let b = block(format!(" {} · Esc closes ", data::line(name)), true);
        let inner = b.inner(rect);
        frame.render_widget(b, rect);
        frame.render_widget(Image::new(image), inner);
        return;
    }
    if app.dialog.is_some() {
        popup(frame, app, area)
    }
}
fn popup(frame: &mut Frame, app: &mut App, area: Rect) {
    let width = area.width.saturating_sub(8).min(100);
    let height = area.height.saturating_sub(6).max(10);
    let rect = centered(area, width, height);
    app.popup_rect = rect;
    frame.render_widget(Clear, rect);
    let known_statuses = app.known_statuses().join(", ");
    let Some(dialog) = &mut app.dialog else {
        return;
    };
    match dialog {
        Dialog::Picker {
            title,
            choices,
            selected,
            ..
        } => {
            let b = block(format!(" {} ", data::line(title)), true);
            let inner = b.inner(rect);
            frame.render_widget(b, rect);
            let rows = inner.height.saturating_sub(2) as usize;
            app.picker_start = selected.saturating_sub(rows.saturating_sub(1));
            for (row, (index, c)) in choices
                .iter()
                .enumerate()
                .skip(app.picker_start)
                .take(rows)
                .enumerate()
            {
                let line = format!(
                    "{} {}",
                    if index == *selected { "›" } else { " " },
                    data::line(&c.label)
                );
                frame.render_widget(
                    Paragraph::new(line).style(if index == *selected {
                        selection()
                    } else {
                        Style::default()
                    }),
                    Rect::new(inner.x, inner.y + 1 + row as u16, inner.width, 1),
                );
            }
            frame.render_widget(
                Paragraph::new(" ↑↓ select · Enter / click · Esc cancels")
                    .style(Style::default().fg(Color::DarkGray)),
                Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            );
        }
        Dialog::Form {
            title,
            labels,
            values,
            selected,
            kind,
            ..
        } => {
            let b = block(format!(" {} ", data::line(title)), true);
            let inner = b.inner(rect);
            frame.render_widget(b, rect);
            let available = (inner.height.saturating_sub(5) / 3).max(1) as usize;
            let start = selected.saturating_sub(available - 1);
            for (row, index) in (start..values.len().min(start + available)).enumerate() {
                let y = inner.y + row as u16 * 3;
                frame.render_widget(
                    Paragraph::new(labels.get(index).map(String::as_str).unwrap_or("Value"))
                        .style(Style::default().fg(Color::DarkGray)),
                    Rect::new(inner.x, y, inner.width, 1),
                );
                let value = format!(
                    "{} {}",
                    if index == *selected { "›" } else { " " },
                    data::safe(&values[index])
                );
                frame.render_widget(
                    Paragraph::new(value.clone()).style(if index == *selected {
                        selection()
                    } else {
                        Style::default()
                    }),
                    Rect::new(inner.x, y + 1, inner.width, 1),
                );
                if index == *selected {
                    frame.set_cursor_position((
                        inner.x + (value.width() as u16).min(inner.width.saturating_sub(1)),
                        y + 1,
                    ));
                }
            }
            if kind == "scope" {
                let hint = format!("Known statuses: {}", known_statuses);
                frame.render_widget(
                    Paragraph::new(hint)
                        .wrap(Wrap { trim: true })
                        .style(Style::default().fg(Color::DarkGray)),
                    Rect::new(inner.x, inner.bottom().saturating_sub(4), inner.width, 2),
                );
            }
            frame.render_widget(
                Paragraph::new(" Tab next · Ctrl+U clears · Ctrl+S saves · Esc cancels")
                    .style(Style::default().fg(Color::DarkGray)),
                Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            );
        }
        Dialog::Editor {
            title,
            value,
            cursor,
            ..
        } => {
            let b = block(format!(" {} ", data::line(title)), true);
            let inner = b.inner(rect);
            frame.render_widget(b, rect);
            let (lines, cx, cy) = crate::input::lines(value, *cursor, inner.width);
            let offset = cy.saturating_sub(inner.height.saturating_sub(3) as usize);
            frame.set_cursor_position((inner.x + cx, inner.y + (cy - offset) as u16));
            frame.render_widget(
                Paragraph::new(lines.into_iter().skip(offset).collect::<Vec<_>>()),
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(2),
                ),
            );
            frame.render_widget(
                Paragraph::new(" Ctrl+S reviews Markdown update · Esc cancels")
                    .style(Style::default().fg(Color::DarkGray)),
                Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            );
        }
        Dialog::Confirm(action) => {
            let b = block(format!(" Confirm {} · {} ", action.kind, action.key), true);
            let inner = b.inner(rect);
            frame.render_widget(b, rect);
            let preview = if action.value.chars().count() > 1500 {
                action.value.chars().take(1500).collect::<String>() + "…"
            } else {
                action.value.clone()
            };
            let ticket = data::Ticket::default();
            let mut lines = markdown::render(&preview, inner.width, &ticket)
                .into_iter()
                .map(|l| l.line())
                .collect::<Vec<_>>();
            if !action.extra.is_empty() {
                lines.push(Line::from(format!(
                    "Link type: {}",
                    data::line(&action.extra)
                )))
            }
            frame.render_widget(
                Paragraph::new(lines),
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(2),
                ),
            );
            frame.render_widget(
                Paragraph::new(" y confirms this ONE ticket · n / Esc cancels").style(
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                ),
                Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            );
        }
        Dialog::Info {
            title,
            text,
            scroll,
        } => {
            let b = block(format!(" {} ", data::line(title)), true);
            let inner = b.inner(rect);
            frame.render_widget(b, rect);
            frame.render_widget(
                Paragraph::new(data::safe(text))
                    .wrap(Wrap { trim: false })
                    .scroll((*scroll, 0)),
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(2),
                ),
            );
            frame.render_widget(
                Paragraph::new(" ↑↓ scroll · Enter / Esc closes")
                    .style(Style::default().fg(Color::DarkGray)),
                Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backend::Backend,
        config::{Cache, Settings},
    };
    use ratatui::{Terminal, backend::TestBackend};
    #[test]
    fn two_panes_and_top_views_fit() {
        for (w, h) in [(60, 16), (82, 26), (120, 24), (163, 28), (180, 56)] {
            let settings = Settings {
                project: "DEV".into(),
                last_view: 2,
                ..Default::default()
            };
            let mut app = App::new(
                Backend::new("demo", None).unwrap(),
                Cache { directory: None },
                settings,
                true,
                500,
                std::path::PathBuf::new(),
            );
            app.items = data::demo_tickets();
            app.visible = (0..app.items.len()).collect();
            app.table.select(Some(0));
            app.detail_key = "DEV-101".into();
            for t in data::demo_tickets() {
                app.details.insert(t.key.clone(), (t, crate::config::now()));
            }
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            assert!(app.tickets_rect.width > 0 && app.ticket_rect.width > 0);
            assert_eq!(app.tabs.len(), 4);
            assert!(
                app.document
                    .iter()
                    .flat_map(|l| &l.chunks)
                    .any(|c| c.style.add_modifier.contains(Modifier::BOLD))
            );
        }
    }
}
