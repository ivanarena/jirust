//! Synthetic-only end-to-end search benchmark: cargo run --release --example search_bench
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use jirust::{
    app::{App, Message},
    backend::Backend,
    config::{Cache, Settings},
    data, render,
    workspace::Workspace,
};
use ratatui::{Terminal, backend::TestBackend};
use serde_json::json;
use std::{path::PathBuf, time::Instant};
fn main() {
    let count = 1000;
    let mut workspace = Workspace::default();
    workspace.complete = true;
    for i in 0..count {
        let mut ticket = data::demo_tickets().remove(0);
        ticket.key = format!("DEV-{}", 1000 + i);
        let text = "Synthetic description with Unicode 日本語 and ordinary words. ".repeat(8);
        ticket.fields["description"] = json!({"type":"doc","content":(0..32).map(|_|json!({"type":"paragraph","content":[{"type":"text","text":text}]})).collect::<Vec<_>>()});
        ticket.fields["summary"] = json!(format!(
            "Synthetic {} {}",
            i,
            if i % 20 == 0 { "needle" } else { "ordinary" }
        ));
        workspace.items.push(ticket);
    }
    let start = Instant::now();
    for ticket in &workspace.items {
        std::hint::black_box(
            format!("{} {}", ticket.summary(), ticket.description())
                .to_lowercase()
                .contains("needle"),
        );
    }
    let old_scan = start.elapsed();
    workspace.reindex();
    let mut app = App::new(
        Backend::new("demo", None).unwrap(),
        Cache { directory: None },
        Settings {
            project: "DEV".into(),
            prefix: "DEV".into(),
            last_view: jirust::config::BACKLOG,
            ..Default::default()
        },
        true,
        500,
        PathBuf::new(),
    );
    let _ = app.reload();
    app.message(Message::Synced {
        generation: app.generation,
        result: Ok((workspace, vec![])),
    });
    let mut terminal = Terminal::new(TestBackend::new(160, 40)).unwrap();
    terminal.draw(|f| render::draw(f, &mut app)).unwrap();
    let mut filters = Vec::new();
    let mut frames = Vec::new();
    for _ in 0..10 {
        app.event(Event::Key(KeyEvent::new(
            KeyCode::Char('/'),
            KeyModifiers::NONE,
        )));
        for c in "needle".chars() {
            let start = Instant::now();
            assert!(
                app.event(Event::Key(KeyEvent::new(
                    KeyCode::Char(c),
                    KeyModifiers::NONE
                )))
                .is_empty()
            );
            filters.push(start.elapsed());
            let start = Instant::now();
            terminal.draw(|f| render::draw(f, &mut app)).unwrap();
            frames.push(start.elapsed());
            assert!(app.search_due.is_none());
        }
        assert_eq!(
            app.visible.len(),
            50,
            "query={}, local={}, remote={}, view={}, assignee={}, status={}",
            app.search,
            app.local_search,
            app.remote_search,
            app.view,
            app.assignee_filter,
            app.status_filter
        );
        app.event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
    }
    filters.sort();
    frames.sort();
    println!("{count} synthetic tickets × 32 ADF paragraphs");
    println!(
        "Old formatting scan: {:.2} ms",
        old_scan.as_secs_f64() * 1000.0
    );
    println!(
        "Indexed input: median {:.3} ms, worst {:.3} ms",
        filters[filters.len() / 2].as_secs_f64() * 1000.0,
        filters.last().unwrap().as_secs_f64() * 1000.0
    );
    println!(
        "Terminal frame: median {:.3} ms, worst {:.3} ms",
        frames[frames.len() / 2].as_secs_f64() * 1000.0,
        frames.last().unwrap().as_secs_f64() * 1000.0
    );
}
