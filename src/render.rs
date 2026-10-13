use crate::{
    app::{App, Dialog, Focus, InlineImageState, SearchMode},
    appearance, data, markdown,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
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
            Paragraph::new("jirust needs 60×16 cells. Resize/zoom this pane. q quits."),
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
        " jirust {} ",
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
        let label = if area.width < 72 {
            format!(" ^{}{} ", index + 1, data::line(&name))
        } else {
            format!(" ^{} {} ", index + 1, data::line(&name))
        };
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
            Constraint::Length(24),
            Constraint::Length(23),
        ])
        .split(rows[1]);
    app.search_rect = controls[0];
    app.status_rect = controls[1];
    app.assignee_rect = controls[2];
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
                format!(
                    " / Search {} · digits find any ticket",
                    app.view_names()[app.view]
                )
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
        Paragraph::new(format!(" Assignee: {} ▾", app.assignee_label()))
            .style(Style::default().fg(Color::Blue)),
        app.assignee_rect,
    );
    frame.render_widget(
        Paragraph::new(format!(
            " Status: {} ▾",
            if app.status_filter.is_empty() {
                "Any".into()
            } else if app.status_filter.len() > 1 {
                format!("{} selected", app.status_filter.len())
            } else {
                appearance::status_label(app.status_filter.single())
            }
        ))
        .style(if app.status_filter.len() == 1 {
            app.status_colors.style_name(app.status_filter.single())
        } else {
            Style::default().fg(Color::Blue)
        }),
        app.status_rect,
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
            if app.loading {
                " · loading"
            } else if app.syncing {
                " · refreshing"
            } else {
                ""
            }
        ),
        app.focus == Focus::Tickets,
    );
    let inner = left_block.inner(panes[0]);
    app.row_height = if inner.width < 65 { 2 } else { 1 };
    let table = if app.row_height == 1 {
        let header = Row::new(["Key", "Status", "Assignee", "SP", "Summary"])
            .style(Style::default().fg(Color::DarkGray));
        let data_rows = app.visible.iter().enumerate().map(|(position, i)| {
            let t = &app.items[*i];
            Row::new(vec![
                Cell::from(t.key.clone()),
                Cell::from(appearance::status_label(&t.status())).style(app.status_colors.style(t)),
                Cell::from(t.user("assignee").display_label()),
                Cell::from(t.points_label()),
                Cell::from(appearance::ticket_summary_with_style(
                    t,
                    inner.width.saturating_sub(46),
                    app.epic_colors.style(t),
                )),
            ])
            .style(if position == app.selected {
                selection()
            } else {
                Style::default()
            })
        });
        Table::new(
            data_rows,
            [
                Constraint::Length(11),
                Constraint::Length(13),
                Constraint::Length(14),
                Constraint::Length(4),
                Constraint::Min(12),
            ],
        )
        .header(header)
    } else {
        let header = Row::new(["Key", "Ticket / Status · SP · Assignee"])
            .style(Style::default().fg(Color::DarkGray));
        let data_rows = app.visible.iter().enumerate().map(|(position, i)| {
            let t = &app.items[*i];
            Row::new(vec![
                Cell::from(t.key.clone()),
                Cell::from(Text::from(vec![
                    appearance::ticket_summary_with_style(
                        t,
                        inner.width.saturating_sub(12),
                        app.epic_colors.style(t),
                    ),
                    Line::from(vec![
                        Span::styled(
                            appearance::status_label(&t.status()),
                            app.status_colors.style(t),
                        ),
                        Span::styled(
                            format!(
                                "{}{}",
                                {
                                    let points = t.points_label();
                                    if points.is_empty() {
                                        String::new()
                                    } else {
                                        format!(" · SP:{points}")
                                    }
                                },
                                {
                                    let name = t.user("assignee").display_label();
                                    if name.is_empty() {
                                        String::new()
                                    } else {
                                        format!(" · {name}")
                                    }
                                }
                            ),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]),
                ])),
            ])
            .style(if position == app.selected {
                selection()
            } else {
                Style::default()
            })
            .height(2)
        });
        Table::new(data_rows, [Constraint::Length(11), Constraint::Min(20)]).header(header)
    }
    .column_spacing(1)
    // Rows already paint the selection background; a final table-wide
    // background override would erase each selected status badge's color.
    .row_highlight_style(Style::default().add_modifier(Modifier::BOLD))
    .block(left_block);
    frame.render_stateful_widget(table, panes[0], &mut app.table);
    if app.visible.is_empty() && !app.loading {
        let count = app
            .workspace
            .as_ref()
            .map(|w| w.view_count(app.view))
            .unwrap_or(0);
        let message = if count > 0 {
            format!(
                "No tickets match these filters.\n{count} tickets are in {}.\nF6 / x clears filters and search.",
                app.view_names()[app.view]
            )
        } else {
            "No tickets in this view.\nF5 refreshes; f reviews filters.".into()
        };
        frame.render_widget(
            Paragraph::new(message)
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(Color::DarkGray)),
            Rect::new(
                inner.x,
                inner.y + 1,
                inner.width,
                inner.height.saturating_sub(1),
            ),
        );
    }
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
        frame.render_widget(Paragraph::new(lines), right);
        for slot in &app.inline_slots {
            if slot.row + slot.height as usize <= app.scroll
                || slot.row >= app.scroll + right.height as usize
            {
                continue;
            }
            let y = right.y + slot.row.saturating_sub(app.scroll) as u16;
            let visible_height = (slot.row + slot.height as usize)
                .min(app.scroll + right.height as usize)
                - slot.row.max(app.scroll);
            if slot.row > app.scroll
                && let Some(InlineImageState::Ready(image)) =
                    app.inline_images.get(&slot.attachment.id)
            {
                let mode = image.mode;
                frame.render_widget(
                    Paragraph::new(format!("▧ {} · {mode}", data::line(&slot.attachment.name)))
                        .style(Style::default().fg(Color::Blue)),
                    Rect::new(right.x, y.saturating_sub(1), right.width, 1),
                );
            }
            let rect = Rect::new(right.x, y, right.width, visible_height as u16);
            if let Some(InlineImageState::Ready(image)) = app.inline_images.get(&slot.attachment.id)
            {
                frame.render_widget(Clear, rect);
                let offset = (slot.row as i64 - app.scroll as i64)
                    .clamp(i16::MIN as i64, i16::MAX as i64) as i16;
                frame.render_widget(
                    ratatui_image::sliced::SlicedImage::new(&image.protocol, (0, offset).into()),
                    right,
                );
            } else {
                let caption = match app.inline_images.get(&slot.attachment.id) {
                    Some(InlineImageState::Failed(e)) => {
                        format!("Image unavailable: {e} · i retries / l opens attachments")
                    }
                    _ => "Loading full-width image… (read-only)".into(),
                };
                frame.render_widget(
                    Paragraph::new(caption)
                        .wrap(Wrap { trim: true })
                        .style(Style::default().fg(Color::DarkGray)),
                    rect,
                );
            }
        }
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
        ('r', "Refresh"),
        ('x', "Clear filters"),
        ('a', "Assign"),
        ('t', "Status"),
        ('e', "Edit"),
        ('c', "Comment"),
        ('i', "Images"),
        ('l', "Links"),
        ('O', "Jira"),
        ('f', "Filters"),
        ('?', "Help"),
    ];
    let mut bx = rows[4].x;
    for (key, label) in buttons {
        let text = if key == 'r' {
            format!(" F5 {label} ")
        } else if key == 'x' {
            format!(" F6 {label} ")
        } else {
            format!(" {key} {label} ")
        };
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
    let margin = if matches!(app.dialog, Some(Dialog::Filters { .. })) {
        2
    } else {
        6
    };
    let height = area.height.saturating_sub(margin).max(10);
    let rect = centered(area, width, height);
    app.popup_rect = rect;
    frame.render_widget(Clear, rect);
    let Some(dialog) = &mut app.dialog else {
        return;
    };
    match dialog {
        Dialog::Filters {
            choices,
            selected,
            active,
            checked,
            custom,
        } => {
            let b = block(
                format!(
                    " {} filters · Status → Assignee ",
                    crate::config::VIEW_NAMES[app.view]
                ),
                true,
            );
            let inner = b.inner(rect);
            frame.render_widget(b, rect);
            let columns =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(
                    Rect::new(
                        inner.x,
                        inner.y,
                        inner.width,
                        inner.height.saturating_sub(3),
                    ),
                );
            for column in 0..2 {
                let b = block(
                    format!(
                        " {} · {} selected · {}/{} ",
                        if column == 0 { "Assignees" } else { "Statuses" },
                        checked[column].len(),
                        selected[column] + 1,
                        choices[column].len()
                    ),
                    *active == column,
                );
                let list = b.inner(columns[1 - column]);
                frame.render_widget(b, columns[1 - column]);
                app.filter_list_rects[column] = list;
                let start =
                    selected[column].saturating_sub((list.height as usize).saturating_sub(1));
                app.filter_list_starts[column] = start;
                for (y, (index, choice)) in choices[column]
                    .iter()
                    .enumerate()
                    .skip(start)
                    .take(list.height as usize)
                    .enumerate()
                {
                    let chosen = index == selected[column];
                    let style =
                        if column == 1 && !choice.value.is_empty() && choice.value != "__other" {
                            let badge = app.status_colors.style_name(&choice.value);
                            if chosen && *active == column {
                                badge.bg(Color::Rgb(215, 232, 248))
                            } else {
                                badge
                            }
                        } else if chosen && *active == column {
                            selection()
                        } else if chosen {
                            Style::default()
                                .fg(Color::Blue)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        };
                    frame.render_widget(
                        Paragraph::new(format!(
                            "{} [{}] {}",
                            if chosen { "›" } else { " " },
                            if choice.value == "__other" {
                                "+"
                            } else if checked[column].contains(&choice.value) {
                                "x"
                            } else {
                                " "
                            },
                            if column == 1
                                && matches!(
                                    choice.value.to_ascii_lowercase().as_str(),
                                    "done" | "discarded"
                                )
                            {
                                format!(
                                    "{} {}",
                                    appearance::status_label(&choice.value),
                                    data::line(&choice.label)
                                )
                            } else {
                                data::line(&choice.label)
                            }
                        ))
                        .style(style),
                        Rect::new(list.x, list.y + y as u16, list.width, 1),
                    );
                }
            }
            if let Some(input) = custom {
                let label = if input.column == 0 {
                    "Add assignee"
                } else {
                    "Add status"
                };
                frame.render_widget(
                    Paragraph::new(format!(
                        "{label}: {}_ · Enter adds / Esc back",
                        data::line(&input.value)
                    ))
                    .style(Style::default().fg(Color::Blue)),
                    Rect::new(inner.x, inner.bottom().saturating_sub(3), inner.width, 1),
                );
            }
            let y = inner.bottom().saturating_sub(2);
            app.filter_apply_rect = Rect::new(inner.x, y, 16.min(inner.width), 1);
            app.filter_reset_rect = Rect::new(
                inner.x + 16.min(inner.width),
                y,
                22.min(inner.width.saturating_sub(16)),
                1,
            );
            frame.render_widget(
                Paragraph::new(" Apply [Enter] ").style(
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                ),
                app.filter_apply_rect,
            );
            frame.render_widget(
                Paragraph::new(" Reset both [Ctrl+R] ").style(Style::default().fg(Color::Blue)),
                app.filter_reset_rect,
            );
            frame.render_widget(
                Paragraph::new(" Space/click toggle · Enter apply · Esc cancel")
                    .style(Style::default().fg(Color::DarkGray)),
                Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            );
        }
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
            let preview = if matches!(action.kind.as_str(), "board" | "backlog") {
                format!(
                    "Move {} to {} on board {}.\n\nThis changes membership, not workflow status. Jira determines the destination Rank; refresh will show its authoritative position.",
                    action.key,
                    if action.kind == "board" {
                        "Board"
                    } else {
                        "Backlog"
                    },
                    action.value
                )
            } else if action.value.chars().count() > 1500 {
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
    #[tokio::test]
    async fn mouse_views_are_assignment_filters_without_setup() {
        use crate::{
            app::{Job, Message},
            config,
        };
        use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        let backend = Backend::new("demo", None).unwrap();
        let mut app = App::new(
            backend.clone(),
            Cache { directory: None },
            Settings {
                project: "DEV".into(),
                ..Default::default()
            },
            true,
            500,
            std::path::PathBuf::new(),
        );
        let mut terminal = Terminal::new(TestBackend::new(120, 28)).unwrap();
        let Job::Sync {
            generation,
            base,
            mine,
            changed,
        } = app.reload().into_iter().next().unwrap()
        else {
            panic!()
        };
        let result = backend.workspace(&base, &mine, changed.as_deref()).await;
        app.message(Message::Synced { generation, result });
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        assert_eq!(app.visible.len(), 1);
        assert_eq!(app.assignee_label(), "Me");
        assert_eq!(app.tabs.len(), 4);
        for (view, count, label) in [
            (config::AVAILABLE, 1, ""),
            (config::BOARD, 2, "All"),
            (config::BACKLOG, 1, "All"),
            (config::MINE, 1, "Me"),
        ] {
            let rect = app.tabs[view].0;
            let jobs = app.event(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: rect.x + 1,
                row: rect.y,
                modifiers: KeyModifiers::NONE,
            }));
            assert!(app.dialog.is_none());
            assert!(jobs.is_empty(), "Switching tabs must not contact Jira");
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            assert_eq!(app.view, view);
            assert_eq!(app.visible.len(), count);
            assert_eq!(app.assignee_label(), label);
        }
        app.search_mode = SearchMode::Text;
        app.search = "keep search".into();
        let (rect, key) = app.footer_buttons[0];
        assert_eq!(key, 'r');
        let jobs = app.event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x + 1,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        }));
        assert!(matches!(&jobs[..], [Job::Sync { changed: None, .. }]));
        assert_eq!(app.search, "keep search");
        assert!(matches!(app.search_mode, SearchMode::Text));
    }
    #[test]
    fn multiple_checked_values_are_visible_and_toolbar_shows_selection_counts() {
        use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
        let mut app = App::new(
            Backend::new("demo", None).unwrap(),
            Cache { directory: None },
            Settings {
                project: "DEV".into(),
                last_view: crate::config::BACKLOG,
                ..Default::default()
            },
            true,
            500,
            std::path::PathBuf::new(),
        );
        app.items = data::demo_tickets();
        app.visible = (0..3).collect();
        app.assignee_filter = vec!["@me".into(), "sam".into()].into();
        app.status_filter = vec!["In Progress".into(), "To Do".into()].into();
        for (width, height) in [(60, 16), (120, 28)] {
            app.event(Event::Key(KeyEvent::new(
                KeyCode::Char('f'),
                KeyModifiers::NONE,
            )));
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            let screen = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(screen.contains("[x] Me"));
            assert!(screen.contains("[x] In Progress"));
            assert!(screen.contains("[x] To Do"));
            assert!(screen.contains("Space/click toggle"));
            app.event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            let screen = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(screen.contains("2 selected"));
        }
    }
    #[test]
    fn missing_points_are_blank_and_narrow_rows_omit_empty_sp_labels_but_keep_zero() {
        for width in [60, 210] {
            for (value, label) in [
                (serde_json::Value::Null, ""),
                (serde_json::json!(0), "0"),
                (serde_json::json!(2.5), "2.5"),
            ] {
                let mut app = App::new(
                    Backend::new("demo", None).unwrap(),
                    Cache { directory: None },
                    Settings {
                        project: "DEV".into(),
                        last_view: crate::config::AVAILABLE,
                        ..Default::default()
                    },
                    true,
                    500,
                    std::path::PathBuf::new(),
                );
                app.items = data::demo_tickets();
                app.items[1].fields["customfield_90123"] = value;
                app.visible = vec![1];
                app.detail_key = "DEV-102".into();
                let mut terminal = Terminal::new(TestBackend::new(width, 28)).unwrap();
                terminal.draw(|f| draw(f, &mut app)).unwrap();
                let buffer = terminal.backend().buffer();
                let left = (app.tickets_rect.y + 2..app.tickets_rect.bottom() - 1)
                    .flat_map(|y| {
                        (app.tickets_rect.x + 1..app.tickets_rect.right() - 1).map(move |x| (x, y))
                    })
                    .map(|p| buffer[p].symbol())
                    .collect::<String>();
                if app.row_height == 2 {
                    assert_eq!(left.contains("SP:"), !label.is_empty());
                    if !label.is_empty() {
                        assert!(left.contains(&format!("SP:{label}")));
                    }
                } else {
                    let x = app.tickets_rect.x + 1 + 11 + 1 + 13 + 1 + 14 + 1;
                    let y = app.tickets_rect.y + 2;
                    let cell = (x..x + 4)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>();
                    assert_eq!(cell.trim(), label);
                }
                let doc = crate::markdown::ticket_document(&app.items[1], 80);
                let metadata = doc
                    .iter()
                    .find(|r| r.chunks.iter().any(|c| c.text.trim() == "Story points:"))
                    .unwrap();
                let text = metadata
                    .chunks
                    .iter()
                    .map(|c| c.text.as_str())
                    .collect::<String>();
                assert_eq!(text.trim(), format!("Story points: {label}").trim());
            }
        }
    }
    #[test]
    fn unassigned_fields_are_blank_in_wide_and_narrow_lists_and_details() {
        for width in [60, 210] {
            let mut app = App::new(
                Backend::new("demo", None).unwrap(),
                Cache { directory: None },
                Settings {
                    project: "DEV".into(),
                    last_view: crate::config::AVAILABLE,
                    ..Default::default()
                },
                true,
                500,
                std::path::PathBuf::new(),
            );
            app.items = data::demo_tickets();
            app.visible = vec![1];
            app.detail_key = "DEV-102".into();
            let mut terminal = Terminal::new(TestBackend::new(width, 28)).unwrap();
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            let buffer = terminal.backend().buffer();
            let screen = buffer
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(!screen.contains("Unassigned"));
            assert!(screen.contains("Assignee:"));
            assert!(screen.contains("Reporter:"));
            if app.row_height == 1 {
                let x = app.tickets_rect.x + 1 + 11 + 1 + 13 + 1;
                let y = app.tickets_rect.y + 2;
                assert!((x..x + 14).all(|x| buffer[(x, y)].symbol() == " "));
            }
        }
    }
    #[test]
    fn filter_lists_are_visible_on_small_terminals_and_mouse_selection_is_staged() {
        use crossterm::event::{
            Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        };
        for (width, height) in [(60, 16), (82, 26), (120, 28)] {
            let mut app = App::new(
                Backend::new("demo", None).unwrap(),
                Cache { directory: None },
                Settings {
                    project: "DEV".into(),
                    last_view: crate::config::BACKLOG,
                    ..Default::default()
                },
                true,
                500,
                std::path::PathBuf::new(),
            );
            app.items = data::demo_tickets();
            app.visible = (0..3).collect();
            app.event(Event::Key(KeyEvent::new(
                KeyCode::Char('f'),
                KeyModifiers::NONE,
            )));
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            assert!(app.status_rect.x < app.assignee_rect.x);
            assert!(app.filter_list_rects[1].x < app.filter_list_rects[0].x);
            let screen = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            for label in [
                "Assignees",
                "Statuses",
                "Alex",
                "Sam",
                "In Progress",
                "Refinement",
                "To Do",
            ] {
                assert!(
                    screen.contains(label),
                    "missing {label} at {width}x{height}"
                );
            }
            for (column, index) in [(0, 4), (1, 3)] {
                let rect = app.filter_list_rects[column];
                app.event(Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: rect.x + 1,
                    row: rect.y + index,
                    modifiers: KeyModifiers::NONE,
                }));
            }
            assert!(app.assignee_filter.is_empty());
            assert!(app.status_filter.is_empty());
            let apply = app.filter_apply_rect;
            let jobs = app.event(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: apply.x + 1,
                row: apply.y,
                modifiers: KeyModifiers::NONE,
            }));
            assert!(jobs.iter().all(|j| !matches!(j, crate::app::Job::Write(_))));
            assert_eq!(app.assignee_filter, "sam");
            assert_eq!(app.status_filter, "To Do");
            assert!(app.dialog.is_none());
        }
    }
    #[test]
    fn rendered_ticket_rows_keep_manual_sequence_not_numerical_order() {
        use crate::app::Message;
        let mut app = App::new(
            Backend::new("demo", None).unwrap(),
            Cache { directory: None },
            Settings {
                project: "DEV".into(),
                last_view: crate::config::BACKLOG,
                ..Default::default()
            },
            true,
            500,
            std::path::PathBuf::new(),
        );
        let mut items = data::demo_tickets();
        items[0].key = "DEV-84".into();
        items[1].key = "DEV-97".into();
        items[2].key = "DEV-61".into();
        let mut epic = items[0].clone();
        epic.key = "DEV-1".into();
        epic.fields["issuetype"] = serde_json::json!({"name":"Epic"});
        items.insert(0, epic);
        let workspace = crate::workspace::Workspace {
            items,
            complete: true,
            ..Default::default()
        };
        app.message(Message::Synced {
            generation: app.generation,
            result: Ok((workspace, vec![])),
        });
        let mut terminal = Terminal::new(TestBackend::new(140, 26)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let rows: Vec<String> = (app.tickets_rect.y..app.tickets_rect.bottom())
            .map(|y| {
                (app.tickets_rect.x..app.tickets_rect.right())
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect();
        let order: Vec<_> = rows
            .iter()
            .filter_map(|s| {
                ["DEV-84", "DEV-97", "DEV-61"]
                    .into_iter()
                    .find(|key| s.contains(key))
            })
            .collect();
        assert_eq!(order, ["DEV-84", "DEV-97", "DEV-61"]);
        assert!(
            rows.iter()
                .any(|s| s.contains("DEV-1 ") && s.contains("Epic ·"))
        );
    }
    #[test]
    fn status_badges_keep_colors_on_selected_and_narrow_rows_and_empty_filters_explain_count() {
        for (width, height) in [(60, 16), (140, 26)] {
            let mut app = App::new(
                Backend::new("demo", None).unwrap(),
                Cache { directory: None },
                Settings {
                    project: "DEV".into(),
                    last_view: crate::config::BACKLOG,
                    ..Default::default()
                },
                true,
                500,
                std::path::PathBuf::new(),
            );
            let mut items = data::demo_tickets();
            items[0].fields["status"] =
                serde_json::json!({"name":"Done","statusCategory":{"key":"done"}});
            items[1].fields["status"] =
                serde_json::json!({"name":"Queued","statusCategory":{"key":"new"}});
            items[2].fields["status"] =
                serde_json::json!({"name":"Discarded","statusCategory":{"key":"done"}});
            app.message(crate::app::Message::Synced {
                generation: app.generation,
                result: Ok((
                    crate::workspace::Workspace {
                        items,
                        complete: true,
                        ..Default::default()
                    },
                    vec![],
                )),
            });
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            let colors = app
                .items
                .iter()
                .map(|t| app.status_colors.style(t).bg.unwrap())
                .collect::<Vec<_>>();
            let b = terminal.backend().buffer();
            for color in colors {
                assert!(
                    (app.tickets_rect.y..app.tickets_rect.bottom())
                        .any(|y| (app.tickets_rect.x..app.tickets_rect.right())
                            .any(|x| b[(x, y)].bg == color)),
                    "Missing status color on {width}-column terminal"
                );
            }
            app.last_query = app
                .settings
                .query(crate::config::BACKLOG, "", "", "")
                .unwrap();
            app.status_filter = "Missing status".into();
            app.reload();
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            let screen = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(screen.contains("No tickets match"));
            assert!(screen.contains("3 tickets are in Backlog"));
            assert!(screen.contains("F6 Clear filters"));
        }
    }
    #[test]
    fn epic_tickets_are_plain_and_only_assigned_epic_labels_are_colored_with_status_badges_intact()
    {
        for width in [60, 210] {
            let mut app = App::new(
                Backend::new("demo", None).unwrap(),
                Cache { directory: None },
                Settings {
                    project: "DEV".into(),
                    last_view: crate::config::BACKLOG,
                    ..Default::default()
                },
                true,
                500,
                std::path::PathBuf::new(),
            );
            let mut items = data::demo_tickets();
            items[0].fields["issuetype"] = serde_json::json!({"name":"Epic"});
            items[0].fields["status"] = serde_json::json!({"name":"Done"});
            items[1].fields["issuetype"] = serde_json::json!({"name":"Epic"});
            items[1].fields["status"] = serde_json::json!({"name":"Discarded"});
            items[2].fields["parent"] = serde_json::json!({"key":"DEV-900","fields":{"summary":"Platform","issuetype":{"name":"Epic"}}});
            app.message(crate::app::Message::Synced {
                generation: app.generation,
                result: Ok((
                    crate::workspace::Workspace {
                        items,
                        complete: true,
                        ..Default::default()
                    },
                    vec![],
                )),
            });
            let mut terminal = Terminal::new(TestBackend::new(width, 26)).unwrap();
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            let b = terminal.backend().buffer();
            let badge = app.epic_colors.style_key("DEV-900").bg.unwrap();
            for position in 0..2 {
                let y = app.tickets_rect.y + 2 + position * app.row_height;
                let start = app.tickets_rect.x + if app.row_height == 1 { 47 } else { 13 };
                assert!(
                    (app.tickets_rect.x + 1..app.tickets_rect.x + 12)
                        .chain(start..app.tickets_rect.right() - 1)
                        .all(|x| b[(x, y)].bg != badge),
                    "Epic ticket keys/titles must not have epic-label colors"
                );
            }
            for bg in [
                badge,
                app.status_colors.style_name("Done").bg.unwrap(),
                app.status_colors.style_name("Discarded").bg.unwrap(),
            ] {
                assert!(
                    (app.tickets_rect.y..app.tickets_rect.bottom())
                        .any(|y| (app.tickets_rect.x..app.tickets_rect.right())
                            .any(|x| b[(x, y)].bg == bg)),
                    "Missing parent label/status background at {width} columns"
                );
            }
            let text = b.content.iter().map(|c| c.symbol()).collect::<String>();
            assert!(text.contains("Epic ·"));
            assert!(text.contains("SP"));
            assert!(text.contains("Story points:"));
            app.selected = 2;
            app.table.select(Some(2));
            app.detail_key = "DEV-103".into();
            app.doc_dirty = true;
            terminal.draw(|f| draw(f, &mut app)).unwrap();
            let b = terminal.backend().buffer();
            let y = app.tickets_rect.y + 2 + 2 * app.row_height;
            assert!((app.tickets_rect.x..app.tickets_rect.right()).any(|x| b[(x, y)].bg == badge));
            assert_ne!(
                b[(app.tickets_rect.x + 1, y)].bg,
                badge,
                "Only label, not selected ticket key, should be purple"
            );
        }
    }
    #[test]
    fn terminal_renders_inline_halfblock_images_in_ticket_pane() {
        use crate::app::{InlineImageState, Message};
        use ratatui_image::picker::Picker;
        let mut app = App::new(
            Backend::new("demo", None).unwrap(),
            Cache { directory: None },
            Settings {
                project: "DEV".into(),
                last_view: crate::config::BACKLOG,
                ..Default::default()
            },
            true,
            500,
            std::path::PathBuf::new(),
        );
        let mut t = data::demo_tickets().remove(0);
        t.fields["attachment"] = serde_json::json!([{"id":"23","filename":"synthetic.png","mimeType":"image/png","size":20}]);
        app.message(Message::Synced {
            generation: app.generation,
            result: Ok((
                crate::workspace::Workspace {
                    items: vec![t.clone()],
                    ..Default::default()
                },
                vec![],
            )),
        });
        app.details.insert(t.key.clone(), (t, 0));
        let mut terminal = Terminal::new(TestBackend::new(140, 20)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let slot = app.inline_slots[0].clone();
        app.scroll = slot.row;
        let image = image::DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(
            4,
            8,
            image::Rgb([240, 20, 30]),
        ));
        let p =
            crate::images::make_inline(&Picker::halfblocks(), image, app.inline_size.0).unwrap();
        assert_eq!(p.protocol.size().width, app.inline_size.0);
        assert!(p.protocol.size().height > 10);
        app.inline_images
            .insert("23".into(), InlineImageState::Pending);
        app.message(Message::InlineImage {
            generation: app.inline_generation,
            key: app.detail_key.clone(),
            id: "23".into(),
            result: Ok(p),
        });
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let b = terminal.backend().buffer();
        assert!((app.ticket_rect.y..app.ticket_rect.bottom()).any(|y| {
            (app.ticket_rect.x..app.ticket_rect.right()).any(|x| {
                b[(x, y)].bg == Color::Rgb(240, 20, 30) || b[(x, y)].fg == Color::Rgb(240, 20, 30)
            })
        }));
        app.scroll = app.inline_slots[0].row + 12;
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let right = app.ticket_rect;
        let b = terminal.backend().buffer();
        assert!(
            (right.x + 1..right.right() - 1)
                .all(|x| b[(x, right.y + 1)].bg == Color::Rgb(240, 20, 30)
                    || b[(x, right.y + 1)].fg == Color::Rgb(240, 20, 30)),
            "Scrolled image must still fill the whole ticket pane width"
        );
        let id = app.inline_slots[0].attachment.id.clone();
        let old_width = app.inline_size.0;
        let offset = app.scroll - app.inline_slots[0].row;
        let generation = app.inline_generation;
        terminal.backend_mut().resize(120, 20);
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        assert!(app.inline_generation > generation);
        assert!(app.inline_images.is_empty());
        let slot = app
            .inline_slots
            .iter()
            .find(|s| s.attachment.id == id)
            .unwrap();
        assert_eq!(
            app.scroll,
            slot.row + offset * app.inline_size.0 as usize / old_width as usize
        );
    }
    #[test]
    fn two_panes_and_top_views_fit() {
        for (w, h) in [(60, 16), (82, 26), (120, 24), (163, 28), (180, 56)] {
            let settings = Settings {
                project: "DEV".into(),
                last_view: crate::config::BACKLOG,
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
            let screen = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(screen.contains("F5 Refresh"));
            assert!(
                app.document
                    .iter()
                    .flat_map(|l| &l.chunks)
                    .any(|c| c.style.add_modifier.contains(Modifier::BOLD))
            );
        }
    }
}
