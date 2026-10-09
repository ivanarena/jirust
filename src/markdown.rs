use crate::data::{self, Target, Ticket};
use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use serde_json::{Value, json};
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Debug)]
pub struct Chunk {
    pub text: String,
    pub style: Style,
    pub target: Option<Target>,
}
#[derive(Clone, Debug, Default)]
pub struct DocLine {
    pub chunks: Vec<Chunk>,
}
impl DocLine {
    pub fn line(&self) -> Line<'static> {
        Line::from(
            self.chunks
                .iter()
                .map(|c| Span::styled(c.text.clone(), c.style))
                .collect::<Vec<_>>(),
        )
    }
    pub fn target_at(&self, column: u16) -> Option<Target> {
        let mut x = 0;
        for c in &self.chunks {
            let width = unicode_width::UnicodeWidthStr::width(c.text.as_str()) as u16;
            if column >= x && column < x + width {
                return c.target.clone();
            }
            x += width
        }
        None
    }
}
#[derive(Default)]
struct Format {
    bold: usize,
    italic: usize,
    strike: usize,
    code: bool,
    heading: bool,
    link: Vec<Target>,
}
impl Format {
    fn style(&self) -> Style {
        let mut s = Style::default();
        if self.bold > 0 || self.heading {
            s = s.add_modifier(Modifier::BOLD)
        }
        if self.italic > 0 {
            s = s.add_modifier(Modifier::ITALIC)
        }
        if self.strike > 0 {
            s = s.add_modifier(Modifier::CROSSED_OUT)
        }
        if self.heading {
            s = s.fg(Color::Cyan)
        }
        if self.code {
            s = s.fg(Color::Yellow)
        }
        if !self.link.is_empty() {
            s = s.fg(Color::Blue).add_modifier(Modifier::UNDERLINED)
        }
        s
    }
}
fn append(rows: &mut Vec<DocLine>, text: &str, fmt: &Format, ticket: &Ticket) {
    let style = fmt.style();
    for (n, part) in text.split('\n').enumerate() {
        if n > 0 {
            rows.push(DocLine::default())
        };
        if let Some(target) = fmt.link.last() {
            rows.last_mut().unwrap().chunks.push(Chunk {
                text: part.into(),
                style,
                target: Some(target.clone()),
            });
            continue;
        }
        let mut targets = Vec::new();
        for found in data::URLS.find_iter(part) {
            let url = found.as_str().trim_end_matches(['.', ',', ';', ')', ']']);
            if data::valid_url(url) {
                targets.push((
                    found.start(),
                    found.start() + url.len(),
                    ticket.url_target(url, url),
                ))
            }
        }
        for found in data::KEYS.find_iter(part) {
            if found.as_str() != ticket.key {
                targets.push((
                    found.start(),
                    found.end(),
                    Target::ticket(found.as_str(), found.as_str()),
                ))
            }
        }
        targets.sort_by_key(|(start, _, _)| *start);
        let mut start = 0;
        for (begin, end, target) in targets {
            if begin < start {
                continue;
            }
            if start < begin {
                rows.last_mut().unwrap().chunks.push(Chunk {
                    text: part[start..begin].into(),
                    style,
                    target: None,
                })
            }
            rows.last_mut().unwrap().chunks.push(Chunk {
                text: part[begin..end].into(),
                style: style.fg(Color::Blue).add_modifier(Modifier::UNDERLINED),
                target: Some(target),
            });
            start = end;
        }
        if start < part.len() {
            rows.last_mut().unwrap().chunks.push(Chunk {
                text: part[start..].into(),
                style,
                target: None,
            })
        }
    }
}
fn newline(rows: &mut Vec<DocLine>) {
    if rows.last().is_some_and(|r| !r.chunks.is_empty()) {
        rows.push(DocLine::default())
    }
}
#[derive(Default)]
struct MarkdownTable {
    alignments: Vec<Alignment>,
    rows: Vec<Vec<Vec<DocLine>>>,
    row: Vec<Vec<DocLine>>,
    cell: Vec<DocLine>,
}
fn text_width(line: &DocLine) -> usize {
    line.chunks
        .iter()
        .map(|c| unicode_width::UnicodeWidthStr::width(c.text.as_str()))
        .sum()
}
fn table_lines(table: MarkdownTable, width: usize) -> Vec<DocLine> {
    let count = table.alignments.len();
    if count == 0 || table.rows.is_empty() {
        return vec![];
    }
    let plain = |text: String| Chunk {
        text,
        style: Style::default().fg(Color::DarkGray),
        target: None,
    };
    let budget = width.saturating_sub(3 * count + 1);
    if budget < 2 * count {
        // Too narrow for a readable grid: retain every cell with its header.
        let mut lines = Vec::new();
        if table.rows.len() == 1 {
            return wrap(
                table.rows[0].iter().flatten().cloned().collect(),
                width.max(1),
            );
        }
        for row in table.rows.iter().skip(1) {
            for (column, cell) in row.iter().enumerate() {
                let mut value = table.rows[0]
                    .get(column)
                    .and_then(|h| h.first())
                    .cloned()
                    .unwrap_or_default();
                value.chunks.push(plain(": ".into()));
                let mut cell = cell.clone();
                if !cell.is_empty() {
                    value.chunks.append(&mut cell[0].chunks);
                }
                lines.push(value);
                lines.extend(cell.into_iter().skip(1));
            }
            lines.push(DocLine::default());
        }
        return wrap(lines, width.max(1));
    }
    let mut widths = vec![2; count];
    for row in &table.rows {
        for (column, cell) in row.iter().take(count).enumerate() {
            widths[column] =
                widths[column].max(cell.iter().map(text_width).max().unwrap_or(0).min(budget));
        }
    }
    while widths.iter().sum::<usize>() > budget {
        let column = widths.iter().enumerate().max_by_key(|(_, w)| *w).unwrap().0;
        widths[column] -= 1;
    }
    let border = |left: char, middle: char, right: char| DocLine {
        chunks: vec![plain(format!(
            "{left}{}{right}",
            widths
                .iter()
                .map(|w| "─".repeat(w + 2))
                .collect::<Vec<_>>()
                .join(&middle.to_string())
        ))],
    };
    let mut lines = vec![border('┌', '┬', '┐')];
    for (index, row) in table.rows.iter().enumerate() {
        let cells: Vec<_> = widths
            .iter()
            .enumerate()
            .map(|(column, w)| wrap(row.get(column).cloned().unwrap_or_default(), *w))
            .collect();
        let height = cells.iter().map(Vec::len).max().unwrap_or(1).max(1);
        for y in 0..height {
            let mut line = DocLine {
                chunks: vec![plain("│ ".into())],
            };
            for (column, cell) in cells.iter().enumerate() {
                let content = cell.get(y).cloned().unwrap_or_default();
                let padding = widths[column].saturating_sub(text_width(&content));
                let before = match table.alignments[column] {
                    Alignment::Right => padding,
                    Alignment::Center => padding / 2,
                    _ => 0,
                };
                line.chunks.push(plain(" ".repeat(before)));
                line.chunks.extend(content.chunks);
                line.chunks.push(plain(format!(
                    "{} │{}",
                    " ".repeat(padding - before),
                    if column + 1 < count { " " } else { "" }
                )));
            }
            lines.push(line);
        }
        if index + 1 < table.rows.len() {
            lines.push(border('├', '┼', '┤'));
        }
    }
    lines.push(border('└', '┴', '┘'));
    lines
}
fn html_break(html: &str) -> bool {
    matches!(
        html.trim().to_ascii_lowercase().as_str(),
        "<br>" | "<br/>" | "<br />"
    )
}
pub fn render(md: &str, width: u16, ticket: &Ticket) -> Vec<DocLine> {
    let md = data::safe(md);
    let mut rows = vec![DocLine::default()];
    let mut f = Format::default();
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut table: Option<MarkdownTable> = None;
    for event in Parser::new_ext(&md, Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES) {
        match &event {
            Event::Start(Tag::Table(alignments)) => {
                newline(&mut rows);
                table = Some(MarkdownTable {
                    alignments: alignments.clone(),
                    ..Default::default()
                });
                continue;
            }
            Event::End(TagEnd::Table) => {
                if let Some(table) = table.take() {
                    rows.extend(table_lines(table, width.max(1) as usize));
                    rows.push(DocLine::default());
                }
                continue;
            }
            Event::Start(Tag::TableHead) => {
                f.bold += 1;
                continue;
            }
            Event::Start(Tag::TableRow) => continue,
            Event::End(TagEnd::TableHead | TagEnd::TableRow) => {
                if matches!(event, Event::End(TagEnd::TableHead)) {
                    f.bold = f.bold.saturating_sub(1);
                }
                if let Some(t) = &mut table {
                    t.rows.push(std::mem::take(&mut t.row));
                }
                continue;
            }
            Event::Start(Tag::TableCell) => {
                if let Some(t) = &mut table {
                    t.cell = vec![DocLine::default()];
                }
                continue;
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(t) = &mut table {
                    t.row.push(std::mem::take(&mut t.cell));
                }
                continue;
            }
            _ => {}
        }
        let rows = if let Some(t) = &mut table {
            &mut t.cell
        } else {
            &mut rows
        };
        match event {
            Event::Start(Tag::Heading { .. }) => {
                newline(rows);
                f.heading = true
            }
            Event::End(TagEnd::Heading(_)) => {
                f.heading = false;
                newline(rows);
                rows.push(DocLine::default())
            }
            Event::Start(Tag::Paragraph) => newline(rows),
            Event::End(TagEnd::Paragraph) => {
                newline(rows);
                rows.push(DocLine::default())
            }
            Event::Start(Tag::Strong) => f.bold += 1,
            Event::End(TagEnd::Strong) => f.bold = f.bold.saturating_sub(1),
            Event::Start(Tag::Emphasis) => f.italic += 1,
            Event::End(TagEnd::Emphasis) => f.italic = f.italic.saturating_sub(1),
            Event::Start(Tag::Strikethrough) => f.strike += 1,
            Event::End(TagEnd::Strikethrough) => f.strike = f.strike.saturating_sub(1),
            Event::Start(Tag::Link { dest_url, .. })
            | Event::Start(Tag::Image { dest_url, .. }) => {
                f.link.push(ticket.url_target(&dest_url, &dest_url))
            }
            Event::End(TagEnd::Link) | Event::End(TagEnd::Image) => {
                f.link.pop();
            }
            Event::Start(Tag::CodeBlock(_)) => {
                newline(rows);
                f.code = true
            }
            Event::End(TagEnd::CodeBlock) => {
                f.code = false;
                newline(rows)
            }
            Event::Start(Tag::List(start)) => lists.push(start),
            Event::End(TagEnd::List(_)) => {
                lists.pop();
                newline(rows)
            }
            Event::Start(Tag::Item) => {
                newline(rows);
                let prefix = if let Some(Some(n)) = lists.last_mut() {
                    let s = format!("{n}. ");
                    *n += 1;
                    s
                } else {
                    "• ".into()
                };
                append(
                    rows,
                    &format!("{}{prefix}", "  ".repeat(lists.len().saturating_sub(1))),
                    &f,
                    ticket,
                )
            }
            Event::End(TagEnd::Item) => newline(rows),
            Event::Start(Tag::BlockQuote(_)) => {
                newline(rows);
                append(rows, "│ ", &f, ticket)
            }
            Event::End(TagEnd::BlockQuote(_)) => newline(rows),
            Event::InlineHtml(ref s) if html_break(s) => rows.push(DocLine::default()),
            Event::Text(s) | Event::Html(s) | Event::InlineHtml(s) => append(rows, &s, &f, ticket),
            Event::Code(s) => {
                let old = f.code;
                f.code = true;
                append(rows, &s, &f, ticket);
                f.code = old
            }
            Event::SoftBreak => append(rows, " ", &f, ticket),
            Event::HardBreak => rows.push(DocLine::default()),
            Event::Rule => {
                newline(rows);
                append(rows, "────────────────────────", &f, ticket);
                newline(rows)
            }
            Event::TaskListMarker(checked) => {
                append(rows, if checked { "☑ " } else { "☐ " }, &f, ticket)
            }
            _ => {}
        }
    }
    wrap(rows, width.max(1) as usize)
}
fn wrap(rows: Vec<DocLine>, width: usize) -> Vec<DocLine> {
    fn line(cells: &[(char, Style, Option<Target>)]) -> DocLine {
        let mut out = DocLine::default();
        for (c, s, t) in cells {
            if let Some(last) = out.chunks.last_mut()
                && last.style == *s
                && last.target == *t
            {
                last.text.push(*c);
                continue;
            }
            out.chunks.push(Chunk {
                text: c.to_string(),
                style: *s,
                target: t.clone(),
            })
        }
        out
    }
    let mut out = Vec::new();
    for row in rows {
        let cells: Vec<_> = row
            .chunks
            .into_iter()
            .flat_map(|c| {
                c.text
                    .chars()
                    .map(|ch| (ch, c.style, c.target.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        if cells.is_empty() {
            out.push(DocLine::default());
            continue;
        };
        let mut buffer = Vec::new();
        let mut used = 0;
        for cell in cells {
            let cw = cell.0.width().unwrap_or(0);
            while used + cw > width && !buffer.is_empty() {
                let split = buffer
                    .iter()
                    .rposition(|(c, _, _): &(char, Style, Option<Target>)| c.is_whitespace());
                if let Some(n) = split {
                    out.push(line(&buffer[..n]));
                    buffer = buffer[n + 1..].to_vec();
                    used = buffer.iter().map(|(c, _, _)| c.width().unwrap_or(0)).sum()
                } else {
                    out.push(line(&buffer));
                    buffer.clear();
                    used = 0
                }
            }
            used += cw;
            buffer.push(cell)
        }
        out.push(line(&buffer));
    }
    out
}
pub fn ticket_document(t: &Ticket, width: u16) -> Vec<DocLine> {
    let labels = t.fields["labels"]
        .as_array()
        .map(|a| a.iter().map(data::text).collect::<Vec<_>>().join(", "))
        .unwrap_or_default();
    let md = format!(
        "# {} — {}\n\n**Status:** {}  \n**Assignee:** {}  \n**Reporter:** {}  \n**Type:** {} · **Priority:** {}  \n**Labels:** {}\n\n---\n\n{}\n",
        t.key,
        t.summary(),
        t.status(),
        t.user("assignee").label(),
        t.user("reporter").label(),
        t.kind(),
        t.priority(),
        labels,
        t.description()
    );
    let mut lines = render(&md, width, t);
    let targets = t.targets();
    if !targets.is_empty() {
        lines.extend(render("## Links & attachments", width, t));
        for target in targets {
            let attachment = target.attachment.is_some();
            let text = format!("{} {}", if attachment { "▧" } else { "↗" }, target.label);
            lines.extend(wrap(
                vec![DocLine {
                    chunks: vec![Chunk {
                        text,
                        style: Style::default()
                            .fg(Color::Blue)
                            .add_modifier(Modifier::UNDERLINED),
                        target: Some(target),
                    }],
                }],
                width as usize,
            ))
        }
    }
    if let Some(fields) = t.fields.as_object() {
        let mut customs = Vec::new();
        for (key, v) in fields {
            if key.starts_with("customfield_") && !v.is_null() && v != &json!([]) && v != &json!("")
            {
                let name = t.names[key].as_str().unwrap_or(key);
                let value = if v.is_string() {
                    data::text(v)
                } else {
                    v.to_string()
                };
                customs.push(format!("**{}:** {}", data::line(name), data::line(&value)))
            }
        }
        if !customs.is_empty() {
            lines.extend(render(
                &format!("\n## Custom fields\n\n{}", customs.join("  \n")),
                width,
                t,
            ))
        }
    }
    // Bulk search supplies descriptions before full detail requests finish.
    // Missing comments means "not loaded", not zero comments.
    if t.fields.get("comment").is_none() {
        return lines;
    }
    let comments = t.fields["comment"]["comments"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let total = t.fields["comment"]["total"]
        .as_u64()
        .unwrap_or(comments.len() as u64);
    lines.extend(render(
        &format!("\n## Comments ({} / {total})\n", comments.len()),
        width,
        t,
    ));
    if total > comments.len() as u64 {
        lines.extend(render("Press **C** to load all comments.", width, t))
    }
    for c in comments {
        let author = data::User::from_value(&c["author"]).label();
        lines.extend(render(
            &format!("### {author}\n\n{}\n", data::adf_markdown(&c["body"])),
            width,
            t,
        ))
    }
    lines
}
// Convert the supported Markdown editor constructs to ADF accepted by acli.
// This is data formatting only, not a Jira API client.
pub fn to_adf(md: &str) -> Value {
    fn append(stack: &mut [Value], node: Value) {
        if let Some(parent) = stack.last_mut() {
            parent["content"].as_array_mut().unwrap().push(node)
        }
    }
    fn push(stack: &mut Vec<Value>, kind: &str, attrs: Option<Value>) {
        let mut v = json!({"type":kind,"content":[]});
        if let Some(a) = attrs {
            v["attrs"] = a
        }
        stack.push(v)
    }
    fn pop(stack: &mut Vec<Value>) {
        if stack.len() > 1 {
            let mut node = stack.pop().unwrap();
            node.as_object_mut().unwrap().remove("implicit");
            append(stack, node)
        }
    }
    let mut stack = vec![json!({"type":"doc","version":1,"content":[]})];
    let mut marks: Vec<Value> = Vec::new();
    let mut table_head = false;
    for event in Parser::new_ext(md, Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES) {
        let block_event = matches!(
            &event,
            Event::Start(
                Tag::Paragraph
                    | Tag::Heading { .. }
                    | Tag::BlockQuote(_)
                    | Tag::CodeBlock(_)
                    | Tag::List(_)
                    | Tag::Item
                    | Tag::Table(_)
            ) | Event::End(TagEnd::Item | TagEnd::List(_) | TagEnd::BlockQuote(_))
                | Event::Rule
        );
        if block_event && stack.last().is_some_and(|n| n["implicit"] == true) {
            pop(&mut stack)
        }
        let inline_event = matches!(
            &event,
            Event::Text(_)
                | Event::Code(_)
                | Event::Html(_)
                | Event::InlineHtml(_)
                | Event::SoftBreak
                | Event::HardBreak
        );
        if inline_event
            && stack.last().is_some_and(|n| {
                matches!(n["type"].as_str(), Some("doc" | "listItem" | "blockquote"))
            })
        {
            push(&mut stack, "paragraph", None);
            stack.last_mut().unwrap()["implicit"] = json!(true);
        }
        match event {
            Event::Start(Tag::Paragraph) => push(&mut stack, "paragraph", None),
            Event::Start(Tag::Heading { level, .. }) => {
                push(&mut stack, "heading", Some(json!({"level":level as u8})))
            }
            Event::Start(Tag::BlockQuote(_)) => push(&mut stack, "blockquote", None),
            Event::Start(Tag::CodeBlock(kind)) => {
                let language = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(s) => s.into_string(),
                    _ => String::new(),
                };
                push(&mut stack, "codeBlock", Some(json!({"language":language})))
            }
            Event::Start(Tag::List(n)) => push(
                &mut stack,
                if n.is_some() {
                    "orderedList"
                } else {
                    "bulletList"
                },
                n.map(|order| json!({"order":order})),
            ),
            Event::Start(Tag::Item) => push(&mut stack, "listItem", None),
            Event::Start(Tag::Table(_)) => push(&mut stack, "table", None),
            Event::Start(Tag::TableHead) => {
                table_head = true;
                push(&mut stack, "tableRow", None)
            }
            Event::Start(Tag::TableRow) => push(&mut stack, "tableRow", None),
            Event::Start(Tag::TableCell) => {
                push(
                    &mut stack,
                    if table_head {
                        "tableHeader"
                    } else {
                        "tableCell"
                    },
                    None,
                );
                push(&mut stack, "paragraph", None)
            }
            Event::End(TagEnd::TableCell) => {
                pop(&mut stack);
                pop(&mut stack)
            }
            Event::End(TagEnd::TableHead) => {
                pop(&mut stack);
                table_head = false
            }
            Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::BlockQuote(_)
                | TagEnd::CodeBlock
                | TagEnd::List(_)
                | TagEnd::Item
                | TagEnd::Table
                | TagEnd::TableRow,
            ) => pop(&mut stack),
            Event::Start(Tag::Strong) => marks.push(json!({"type":"strong"})),
            Event::Start(Tag::Emphasis) => marks.push(json!({"type":"em"})),
            Event::Start(Tag::Strikethrough) => marks.push(json!({"type":"strike"})),
            Event::Start(Tag::Link { dest_url, .. })
            | Event::Start(Tag::Image { dest_url, .. }) => {
                marks.push(json!({"type":"link","attrs":{"href":dest_url.to_string()}}))
            }
            Event::End(
                TagEnd::Strong
                | TagEnd::Emphasis
                | TagEnd::Strikethrough
                | TagEnd::Link
                | TagEnd::Image,
            ) => {
                marks.pop();
            }
            Event::InlineHtml(ref s) if html_break(s) => {
                append(&mut stack, json!({"type":"hardBreak"}))
            }
            Event::Text(s) | Event::Html(s) | Event::InlineHtml(s) => {
                let mut node = json!({"type":"text","text":data::safe(&s)});
                if !marks.is_empty() {
                    node["marks"] = json!(marks)
                }
                append(&mut stack, node)
            }
            Event::Code(s) => append(
                &mut stack,
                json!({"type":"text","text":s.to_string(),"marks":[{"type":"code"}]}),
            ),
            Event::SoftBreak => append(&mut stack, json!({"type":"text","text":" "})),
            Event::HardBreak => append(&mut stack, json!({"type":"hardBreak"})),
            Event::Rule => append(&mut stack, json!({"type":"rule"})),
            _ => {}
        }
    }
    while stack.len() > 1 {
        pop(&mut stack)
    }
    let mut doc = stack.pop().unwrap();
    if doc["content"].as_array().unwrap().is_empty() {
        doc["content"] = json!([{"type":"paragraph","content":[]}])
    };
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn styled_markdown_and_link_hit_test() {
        let t = data::demo_tickets().remove(0);
        let lines = render("**bold** [docs](https://example.com) DEV-102", 80, &t);
        assert!(
            lines
                .iter()
                .flat_map(|r| &r.chunks)
                .any(|c| c.text == "bold" && c.style.add_modifier.contains(Modifier::BOLD))
        );
        assert!(lines.iter().flat_map(|r| &r.chunks).any(|c| {
            c.target
                .as_ref()
                .is_some_and(|t| t.key.as_deref() == Some("DEV-102"))
        }));
    }
    fn plain(lines: &[DocLine]) -> String {
        lines
            .iter()
            .map(|l| l.line().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }
    #[test]
    fn tables_have_aligned_borders_styled_headers_and_clickable_cells() {
        let t = data::demo_tickets().remove(0);
        let lines = render(
            "Before\n\n| Item | Value |\n| :--- | ---: |\n| **Bold** | DEV-102 |\n| 日本語 | [docs](https://example.com) |\n\nAfter",
            40,
            &t,
        );
        let text = plain(&lines);
        assert!(text.contains('┌'));
        assert!(text.contains('┼'));
        assert!(text.contains('┘'));
        assert!(text.contains("Before"));
        assert!(text.contains("After"));
        let grid: Vec<_> = lines
            .iter()
            .filter(|l| l.line().to_string().starts_with(['┌', '│', '├', '└']))
            .collect();
        assert!(!grid.is_empty());
        let width = text_width(grid[0]);
        assert!(grid.iter().all(|l| text_width(l) == width));
        assert!(width <= 40);
        assert!(
            lines
                .iter()
                .flat_map(|l| &l.chunks)
                .any(|c| c.text.contains("Item") && c.style.add_modifier.contains(Modifier::BOLD))
        );
        for (label, key, url) in [
            ("DEV-102", Some("DEV-102"), None),
            ("docs", None, Some("https://example.com")),
        ] {
            let row = lines
                .iter()
                .find(|l| l.chunks.iter().any(|c| c.text.contains(label)))
                .unwrap();
            let mut column = 0;
            for c in &row.chunks {
                if c.text.contains(label) {
                    let target = row.target_at(column).unwrap();
                    assert_eq!(target.key.as_deref(), key);
                    assert_eq!(target.url.as_deref(), url);
                    break;
                }
                column += unicode_width::UnicodeWidthStr::width(c.text.as_str()) as u16;
            }
        }
    }
    #[test]
    fn narrow_tables_wrap_cells_or_use_labels_without_losing_contents() {
        let t = data::demo_tickets().remove(0);
        for width in [8, 14, 22, 60] {
            let lines = render(
                "| Name | Notes |\n| --- | --- |\n| Alice | First<br>Second |\n| Bob | 日本語日本語 |",
                width,
                &t,
            );
            assert!(lines.iter().all(|l| text_width(l) <= width as usize));
            let text = plain(&lines);
            let columns: Vec<String> = if text.contains('┌') {
                (1..=2)
                    .map(|column| {
                        text.lines()
                            .filter(|s| s.starts_with('│'))
                            .filter_map(|s| s.split('│').nth(column))
                            .collect::<String>()
                    })
                    .collect()
            } else {
                vec![text.clone()]
            };
            let joined: Vec<String> = columns
                .iter()
                .map(|s| s.chars().filter(|c| !c.is_whitespace()).collect())
                .collect();
            for word in ["Alice", "First", "Second"] {
                assert!(
                    joined.iter().any(|s| s.contains(word)),
                    "missing {word} at width {width}: {text}"
                );
            }
            assert!(!text.contains("<br>"));
        }
    }
    #[test]
    fn adf_tables_with_multiple_paragraphs_render_as_tables_and_roundtrip_breaks() {
        let table = to_adf("| Name | Notes |\n| --- | --- |\n| Alice | First<br>Second \\| pipe |");
        let mut adf = table.clone();
        adf["content"][0]["content"][1]["content"][1]["content"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"paragraph","content":[{"type":"text","text":"Third paragraph"}]}));
        let md = data::adf_markdown(&adf);
        let t = data::demo_tickets().remove(0);
        let rendered = plain(&render(&md, 60, &t));
        assert!(rendered.contains('┌'));
        assert!(rendered.contains("First"));
        assert!(rendered.contains("Second | pipe"));
        assert!(rendered.contains("Third paragraph"));
        let roundtrip = to_adf(&md);
        assert_eq!(roundtrip["content"][0]["type"], "table");
        assert!(roundtrip.to_string().contains("hardBreak"));
    }
    #[test]
    fn adf_list_and_table_have_block_containers() {
        let doc = to_adf("- one\n  - nested\n- two\n\n| Head |\n| --- |\n| Cell |");
        assert_eq!(
            doc["content"][0]["content"][0]["content"][0]["type"],
            "paragraph"
        );
        assert_eq!(
            doc["content"][0]["content"][0]["content"][1]["type"],
            "bulletList"
        );
        assert_eq!(
            doc["content"][1]["content"][0]["content"][0]["type"],
            "tableHeader"
        );
        assert_eq!(
            doc["content"][1]["content"][1]["content"][0]["content"][0]["type"],
            "paragraph"
        );
        assert!(!doc.to_string().contains("implicit"));
    }
    #[test]
    fn bare_web_links_are_clickable_and_do_not_become_foreign_tickets() {
        let ticket = data::demo_tickets().remove(0);
        let rows = render("https://other.example/browse/DEV-102", 80, &ticket);
        let target = rows[0].target_at(4).unwrap();
        assert!(target.key.is_none());
        assert_eq!(
            target.url.as_deref(),
            Some("https://other.example/browse/DEV-102")
        );
    }
    #[test]
    fn wrapping_and_adf() {
        let t = data::demo_tickets().remove(0);
        for line in render("A paragraph with **bold words** and 日本語", 12, &t) {
            assert!(unicode_width::UnicodeWidthStr::width(line.line().to_string().as_str()) <= 12)
        }
        let doc = to_adf("# Heading\n\n**bold** and [docs](https://example.com)\n\n- one\n- two");
        assert_eq!(doc["type"], "doc");
        let s = doc.to_string();
        assert!(s.contains("strong"));
        assert!(s.contains("bulletList"));
        assert!(s.contains("href"));
    }
}
