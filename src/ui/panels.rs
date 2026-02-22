use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
};

use crate::app::{ActivePanel, App, PanelState};
use std::collections::HashSet;

// Column widths (in characters)
const COL_SIZE: u16 = 9;   // e.g. "   1.2 KB"
const COL_DATE: u16 = 16;  // e.g. "2024-03-15 14:22"
const COL_PERM: u16 = 9;   // e.g. "rwxr-xr-x"
const COL_PADDING: u16 = 2;

/// Render a single file panel inside the given area.
/// `show_permissions` adds a "rwxr-xr-x" column (used for the remote panel).
pub fn render_panel(
    frame: &mut Frame,
    panel: &PanelState,
    area: Rect,
    is_active: bool,
    label: &str,
    show_permissions: bool,
    marked: &HashSet<usize>,
) {
    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = format!(" {} — {} ", label, panel.path.display());
    let block = Block::default()
        .title(title.as_str())
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Fixed columns: 1 (mark "✓") + 2 (icon) + COL_PADDING*2 (two separators)
    // + COL_SIZE + COL_DATE + 2 (highlight_symbol "► ")
    // Optional: + COL_PADDING + COL_PERM if show_permissions
    let perm_cols = if show_permissions { COL_PADDING + COL_PERM } else { 0 };
    let fixed_cols = 1 + 2 + COL_PADDING * 2 + COL_SIZE + COL_DATE + 2 + perm_cols;
    let name_width = inner.width.saturating_sub(fixed_cols) as usize;

    let items: Vec<ListItem> = panel
        .entries
        .iter()
        .enumerate()
        .map(|(idx, e)| {
            let is_marked = marked.contains(&idx);

            let (icon, base_style) = if e.is_dir {
                ("▶ ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            } else {
                ("  ", Style::default().fg(Color::White))
            };

            // Marked entries get a distinct name style (bright yellow).
            let name_style = if is_marked {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                base_style
            };

            let mark_str = if is_marked { "✓" } else { " " };

            let name = truncate_name(&e.name, name_width);
            let size_str = match e.size {
                Some(s) => format_size(s),
                None => format!("{:>width$}", "", width = COL_SIZE as usize),
            };
            let date_str = match e.modified {
                Some(t) => format_time(t),
                None => format!("{:>width$}", "", width = COL_DATE as usize),
            };

            let mut spans = vec![
                // Mark indicator replaces the icon's first char slot
                Span::styled(
                    mark_str,
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
                Span::styled(icon, base_style),
                Span::styled(
                    format!("{:<width$}", name, width = name_width),
                    name_style,
                ),
                Span::raw("  "),
                Span::styled(size_str, Style::default().fg(Color::Gray)),
                Span::raw("  "),
                Span::styled(date_str, Style::default().fg(Color::DarkGray)),
            ];

            if show_permissions {
                let perm_str = match &e.permissions {
                    Some(p) => format!("  {:>width$}", p, width = COL_PERM as usize),
                    None => format!("  {:>width$}", "", width = COL_PERM as usize),
                };
                spans.push(Span::styled(
                    perm_str,
                    Style::default().fg(Color::DarkGray),
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(panel.selected));

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("► ");

    frame.render_stateful_widget(list, inner, &mut list_state);
}

/// Render both panels side by side.
/// When `app.panels_swapped` is true the remote panel appears on the left and
/// the local panel on the right — purely visual, the data model is unchanged.
pub fn render_panels(frame: &mut Frame, app: &App, area: Rect) {
    let mid = area.width / 2;
    let left_area = Rect { x: area.x,       y: area.y, width: mid,              height: area.height };
    let right_area = Rect { x: area.x + mid, y: area.y, width: area.width - mid, height: area.height };

    let connected = app.is_connected();
    let remote_label = if connected {
        if let Some(ref conn) = app.sftp {
            format!("Remote [{}@{}]", conn.user, conn.host)
        } else {
            "Remote".to_string()
        }
    } else {
        "Remote [nicht verbunden — F9 für Profile]".to_string()
    };

    // Determine which physical area gets which logical panel.
    let (local_area, remote_area) = if app.panels_swapped {
        (right_area, left_area)
    } else {
        (left_area, right_area)
    };

    render_panel(
        frame,
        &app.left,
        local_area,
        app.active == ActivePanel::Left,
        "Local",
        false,
        &app.left.marked.clone(),
    );
    render_panel(
        frame,
        &app.right,
        remote_area,
        app.active == ActivePanel::Right,
        &remote_label,
        connected,
        &app.right.marked.clone(),
    );
}

fn truncate_name(name: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= max_len {
        name.to_string()
    } else {
        // Show as many chars as fit, replace last 3 with "..."
        let cut = max_len.saturating_sub(3);
        let truncated: String = chars[..cut].iter().collect();
        format!("{}...", truncated)
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit_idx = 0;
    while value >= 1024.0 && unit_idx + 1 < UNITS.len() {
        value /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{:>7} B", bytes)
    } else {
        format!("{:>6.1} {}", value, UNITS[unit_idx])
    }
}

/// Format a SystemTime as "YYYY-MM-DD HH:MM" (local time via UTC offset).
fn format_time(t: SystemTime) -> String {
    let secs = match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(_) => return format!("{:>width$}", "—", width = COL_DATE as usize),
    };

    // Compute local offset from TZ environment (simple approach via libc).
    // We use a manual calendar calculation to avoid pulling in chrono.
    let local_secs = secs + local_utc_offset_secs();
    let (year, month, day, hour, min) = secs_to_datetime(local_secs);
    format!("{:04}-{:02}-{:02} {:02}:{:02}", year, month, day, hour, min)
}

/// Returns the local UTC offset in seconds using the C `timezone` global.
fn local_utc_offset_secs() -> i64 {
    // Safe: reads a global set by the OS, no mutation.
    #[cfg(unix)]
    {
        extern "C" {
            fn tzset();
            static timezone: std::ffi::c_long;
        }
        unsafe {
            tzset();
            -(timezone as i64)
        }
    }
    #[cfg(not(unix))]
    {
        0
    }
}

/// Convert a Unix timestamp (already offset to local) into calendar components.
fn secs_to_datetime(secs: i64) -> (i32, u32, u32, u32, u32) {
    const SECS_PER_DAY: i64 = 86400;

    // Floor-divide so that days is always rounded towards -infinity.
    let mut days = secs / SECS_PER_DAY;
    let mut day_secs = secs % SECS_PER_DAY;
    if day_secs < 0 {
        day_secs += SECS_PER_DAY;
        days -= 1;
    }

    // day_secs is now always in 0..86399 — safe to derive time components.
    let hour = (day_secs / 3600) as u32;
    let min  = ((day_secs % 3600) / 60) as u32;

    // Days since 1970-01-01 → Gregorian calendar
    let mut year = 1970i32;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let month_days: &[i64] = if is_leap(year) {
        &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    for &md in month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }

    (year, month, (days + 1) as u32, hour, min)
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}
