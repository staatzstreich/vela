use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::transfer::queue::TransferHandle;

/// Render the function-key hint bar (and optional transfer progress) at the bottom.
/// `connected` controls whether F3-Disconnect is shown.
/// `upload` / `download` are `Some(handle)` while the respective transfer is running.
pub fn render_statusbar(
    frame: &mut Frame,
    area: Rect,
    connected: bool,
    message: Option<&str>,
    upload: Option<&TransferHandle>,
    download: Option<&TransferHandle>,
) {
    if let Some(handle) = upload {
        render_transfer_bar(frame, area, handle, message, TransferKind::Upload);
    } else if let Some(handle) = download {
        render_transfer_bar(frame, area, handle, message, TransferKind::Download);
    } else {
        render_hint_bar(frame, area, connected, message);
    }
}

// ---------------------------------------------------------------------------
// Hint bar (normal mode)
// ---------------------------------------------------------------------------

fn render_hint_bar(frame: &mut Frame, area: Rect, connected: bool, message: Option<&str>) {
    // Split into 2 rows; hints on row 0, status message on row 1.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    // --- Row 0: Function-key hints ---
    let mut hints: Vec<(&str, &str)> = vec![
        ("F1", "Help"),
        ("F2", "Rename"),
        ("F4", "Edit"),
        ("F5", "Upload"),
        ("F6", "Download"),
        ("F7", "MkDir"),
        ("F8", "Delete"),
        ("F9", "Profile"),
        ("!", "Shell"),
        ("^U", "Swap"),
    ];

    if connected {
        hints.push(("F3", "Disconnect"));
    }
    hints.push(("F10", "Quit"));

    let mut spans: Vec<Span> = Vec::new();
    for (key, label) in &hints {
        let key_style = if *key == "F3" && connected {
            Style::default()
                .bg(Color::Red)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        };
        spans.push(Span::styled(format!(" {} ", key), key_style));
        spans.push(Span::styled(
            format!("{} ", label),
            Style::default().fg(Color::White),
        ));
    }

    let hint_line = Line::from(spans);
    let hint_para = Paragraph::new(hint_line).style(Style::default().bg(Color::Black));
    frame.render_widget(hint_para, rows[0]);

    // --- Row 1: Status message (if any) ---
    let msg_text = message.unwrap_or("");
    let msg_line = Line::from(vec![Span::styled(
        format!(" {}", msg_text),
        Style::default().fg(Color::Yellow),
    )]);
    let msg_para = Paragraph::new(msg_line).style(Style::default().bg(Color::Black));
    frame.render_widget(msg_para, rows[1]);
}

// ---------------------------------------------------------------------------
// Transfer progress bar (shared by upload and download)
// ---------------------------------------------------------------------------

enum TransferKind {
    Upload,
    Download,
}

fn render_transfer_bar(
    frame: &mut Frame,
    area: Rect,
    handle: &TransferHandle,
    _message: Option<&str>,
    kind: TransferKind,
) {
    // Read progress without holding the lock for long.
    let (file_name, files_done, files_total, fraction) = {
        let prog = handle.lock().unwrap();
        (
            prog.current_file.clone(),
            prog.files_done,
            prog.files_total,
            prog.overall_fraction(),
        )
    };

    let (verb, bar_color) = match kind {
        TransferKind::Upload => ("Upload", Color::Green),
        TransferKind::Download => ("Download", Color::Cyan),
    };

    // Split the 2-row status area: row 0 = progress bar, row 1 = filename.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    // --- Row 0: Custom block-character progress bar ---
    // Build the bar entirely from styled spans so there is no pixel-height
    // mismatch between the bar background and the text baseline.
    let width = rows[0].width as usize;
    let pct = (fraction * 100.0).round() as u64;
    let label = format!(" {} {}/{} — {}% ", verb, files_done, files_total, pct);

    // Number of filled columns (█) vs empty columns (░).
    let filled = ((fraction * width as f64).round() as usize).min(width);
    let empty = width.saturating_sub(filled);

    // Center the label over the bar.
    let label_len = label.chars().count().min(width);
    let pad_left = (width.saturating_sub(label_len)) / 2;
    let pad_right = width.saturating_sub(label_len + pad_left);

    // Build each column as a styled character.
    // The label is overlaid by replacing bar characters at the label position.
    let mut bar_chars: Vec<char> = std::iter::repeat('█')
        .take(filled)
        .chain(std::iter::repeat('░').take(empty))
        .collect();

    // Overlay the label text onto bar_chars (centred).
    for (i, c) in label.chars().enumerate() {
        let pos = pad_left + i;
        if pos < bar_chars.len() {
            bar_chars[pos] = c;
        }
    }
    // Silence unused-variable warnings for pad_left/pad_right if label is wider
    let _ = (pad_left, pad_right);

    // Split bar_chars into filled and empty regions, annotating each char.
    let filled_str: String = bar_chars[..filled].iter().collect();
    let empty_str: String = bar_chars[filled..].iter().collect();

    let bar_line = Line::from(vec![
        Span::styled(
            filled_str,
            Style::default()
                .fg(Color::Black)
                .bg(bar_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            empty_str,
            Style::default().fg(bar_color).bg(Color::DarkGray),
        ),
    ]);

    frame.render_widget(
        Paragraph::new(bar_line).block(Block::default()),
        rows[0],
    );

    // --- Row 1: Current filename (truncated to fit) ---
    let available = rows[1].width.saturating_sub(2) as usize;
    let detail = if file_name.is_empty() {
        String::new()
    } else {
        let prefix = " → ";
        let budget = available.saturating_sub(prefix.chars().count());
        format!("{}{}", prefix, truncate(&file_name, budget))
    };

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            detail,
            Style::default().fg(Color::White),
        )))
        .style(Style::default().bg(Color::Black)),
        rows[1],
    );
}

/// Truncate a string to `max` chars, appending `…` if needed.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}
