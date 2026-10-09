//! Small Unicode-aware editor; no shell/editor subprocess or clipboard dependency.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::text::Line;
use unicode_width::UnicodeWidthChar;

fn left(s: &str, n: usize) -> usize {
    s[..n]
        .char_indices()
        .next_back()
        .map(|(p, _)| p)
        .unwrap_or(0)
}
fn right(s: &str, n: usize) -> usize {
    s[n..].chars().next().map(|c| n + c.len_utf8()).unwrap_or(n)
}
fn vertical(s: &str, n: usize, down: bool) -> usize {
    let start = s[..n].rfind('\n').map(|n| n + 1).unwrap_or(0);
    let column = s[start..n].chars().count();
    let target = if down {
        let Some(end) = s[n..].find('\n') else {
            return s.len();
        };
        n + end + 1
    } else {
        if start == 0 {
            return 0;
        };
        s[..start - 1].rfind('\n').map(|n| n + 1).unwrap_or(0)
    };
    let end = s[target..]
        .find('\n')
        .map(|n| target + n)
        .unwrap_or(s.len());
    s[target..end]
        .char_indices()
        .nth(column)
        .map(|(n, _)| target + n)
        .unwrap_or(end)
}
pub fn insert(s: &mut String, cursor: &mut usize, text: &str) {
    s.insert_str(*cursor, text);
    *cursor += text.len()
}
pub fn key(s: &mut String, cursor: &mut usize, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Left => *cursor = left(s, *cursor),
        KeyCode::Right => *cursor = right(s, *cursor),
        KeyCode::Up => *cursor = vertical(s, *cursor, false),
        KeyCode::Down => *cursor = vertical(s, *cursor, true),
        KeyCode::Home => {
            *cursor = if ctrl {
                0
            } else {
                s[..*cursor].rfind('\n').map(|n| n + 1).unwrap_or(0)
            }
        }
        KeyCode::End => {
            *cursor = if ctrl {
                s.len()
            } else {
                s[*cursor..]
                    .find('\n')
                    .map(|n| *cursor + n)
                    .unwrap_or(s.len())
            }
        }
        KeyCode::Backspace => {
            let n = left(s, *cursor);
            s.replace_range(n..*cursor, "");
            *cursor = n
        }
        KeyCode::Delete => {
            let end = right(s, *cursor);
            s.replace_range(*cursor..end, "")
        }
        KeyCode::Enter => insert(s, cursor, "\n"),
        KeyCode::Tab => insert(s, cursor, "    "),
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            insert(s, cursor, &c.to_string())
        }
        _ => {}
    }
}
pub fn lines(s: &str, cursor: usize, width: u16) -> (Vec<Line<'static>>, u16, usize) {
    let width = width.max(1);
    let mut rows = Vec::new();
    let mut line = String::new();
    let mut x = 0u16;
    let mut cx = 0;
    let mut cy = 0;
    for (n, c) in s.char_indices() {
        let w = c.width().unwrap_or(0) as u16;
        if c != '\n' && x + w > width {
            rows.push(Line::from(std::mem::take(&mut line)));
            x = 0
        }
        if n == cursor {
            cx = x.min(width - 1);
            cy = rows.len()
        }
        if c == '\n' {
            rows.push(Line::from(std::mem::take(&mut line)));
            x = 0
        } else {
            line.push(c);
            x += w
        }
    }
    if cursor == s.len() {
        if x >= width {
            rows.push(Line::from(std::mem::take(&mut line)));
            x = 0
        }
        cx = x;
        cy = rows.len()
    }
    rows.push(Line::from(line));
    (rows, cx, cy)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_editing_and_navigation() {
        let mut s = "first\n日本語\nlast".to_string();
        let mut p = s.len();
        key(
            &mut s,
            &mut p,
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
        );
        assert_eq!(p, "first\n日本語".len());
        key(
            &mut s,
            &mut p,
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
        );
        insert(&mut s, &mut p, "X");
        assert!(s.contains("日本X語"));
        key(
            &mut s,
            &mut p,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        assert!(s.contains("日本語"));
        let (rows, x, y) = lines(&s, p, 4);
        assert!(y < rows.len());
        assert!(x < 4)
    }
}
