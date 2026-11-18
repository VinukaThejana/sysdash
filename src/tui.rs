//! The main stats as text. Two ways in:
//! - `sysdash tui`: a live full-screen view for an SSH terminal or the machine's own console.
//!   It runs its own sampler. Quit with q, Esc or Ctrl-C.
//! - [`render_text`]: the same screen drawn once into an off-screen buffer and turned into ANSI
//!   text, which the web server answers to `curl` and other clients that don't ask for HTML.
//!
//! Both reuse the web dashboard's view models, so every number is formatted as on the page.

use std::time::{Duration, Instant};

use ratatui::{
    Frame, Terminal,
    backend::TestBackend,
    buffer::Buffer,
    crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    symbols,
    widgets::{LineGauge, Paragraph, RenderDirection, Sparkline},
};

use crate::{
    agents::{Agent, Agents},
    metrics::{self, Snapshot},
    systemd::{Failing, Systemd},
    views::{Band, Meter, Overview, agent_label},
};

/// The colours the text view uses, one per subsystem plus failure, warning and dim text.
pub struct Palette {
    cpu: Color,
    mem: Color,
    disk: Color,
    net: Color,
    fail: Color,
    warn: Color,
    dim: Color,
    /// Light up as little of the screen as possible: no track behind gauges, shorter history.
    /// For LCD TVs, which raise their backlight (and grey their black) as more pixels light.
    quiet: bool,
}

impl Palette {
    /// The terminal's own 16-colour palette, so the view follows its theme (light or dark,
    /// Catppuccin, Solarized, the Linux console, …). The default.
    pub const ANSI: Self = Self {
        cpu: Color::Blue,
        mem: Color::Green,
        disk: Color::Magenta,
        net: Color::Cyan,
        fail: Color::Red,
        warn: Color::Yellow,
        dim: Color::DarkGray,
        quiet: false,
    };

    /// [`Palette::ANSI`] for the Linux console, where the view usually runs on a TV.
    pub const CONSOLE: Self = Self { quiet: true, ..Self::ANSI };

    /// The web dashboard's exact pastels as 24-bit colour. Ignores the terminal's theme, and
    /// fades on light backgrounds.
    pub const TRUECOLOR: Self = Self {
        cpu: Color::Rgb(0x8e, 0xc5, 0xee),
        mem: Color::Rgb(0x9f, 0xd8, 0x9a),
        disk: Color::Rgb(0xc6, 0xbf, 0xe2),
        net: Color::Rgb(0xff, 0xc4, 0x8f),
        fail: Color::Rgb(0xff, 0x6a, 0x5f),
        warn: Color::Rgb(0xff, 0xe0, 0x8a),
        dim: Color::DarkGray,
        quiet: false,
    };
}

/// The live view's footer.
const LIVE_FOOTER: &str = "q quit · refreshes every 2 s · same data as the web dashboard";
/// Rows the off-screen render gets; trailing blank rows are trimmed afterwards.
const TEXT_ROWS: u16 = 60;

/// How often the failing-services list is re-read; stats redraw every [`FRAME`].
const FAILING_EVERY: Duration = Duration::from_secs(5);
const FRAME: Duration = Duration::from_millis(250);

/// Runs until the user quits. Restores the terminal on the way out, panics included.
pub async fn run(p: &Palette) -> std::io::Result<()> {
    let snapshot = metrics::spawn();
    let systemd = Systemd::connect().await;
    let agent_source = Agents::new();
    let mut agents: Vec<Agent> = Vec::new();

    let mut terminal = ratatui::init();
    let mut failing: Vec<Failing> = Vec::new();
    let mut failing_read: Option<Instant> = None;
    let result = loop {
        match quit_pressed() {
            Ok(false) => {}
            Ok(true) => break Ok(()),
            Err(e) => break Err(e),
        }
        if failing_read.is_none_or(|t| t.elapsed() >= FAILING_EVERY) {
            failing = systemd.failing().await;
            agents = agent_source.list(&snapshot.borrow().clone()).await.agents;
            failing_read = Some(Instant::now());
        }
        let s = snapshot.borrow().clone();
        if let Err(e) = terminal.draw(|f| draw(f, &s, &failing, &agents, LIVE_FOOTER, p)) {
            break Err(e);
        }
        tokio::time::sleep(FRAME).await;
    };
    ratatui::restore();
    result
}

/// Drains pending input without waiting; true on q, Esc or Ctrl-C. Other keys and resizes
/// are dropped (the next draw picks up a new size by itself).
fn quit_pressed() -> std::io::Result<bool> {
    while event::poll(Duration::ZERO)? {
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && (matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The screen drawn once as text, `width` columns wide: ANSI colours when `color`, plain
/// characters otherwise. Trailing spaces and blank rows are trimmed.
pub fn render_text(
    s: &Snapshot,
    failing: &[Failing],
    agents: &[Agent],
    width: u16,
    color: bool,
    footer: &str,
    p: &Palette,
) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, TEXT_ROWS)).expect("an off-screen terminal can't fail");
    terminal.draw(|f| draw(f, s, failing, agents, footer, p)).expect("an off-screen terminal can't fail");
    let text = to_ansi(terminal.backend().buffer(), color);
    let trimmed = text.trim_end_matches('\n');
    format!("{trimmed}\n\n")
}

/// Turns a drawn buffer into lines of text, with SGR colour codes when `color`.
fn to_ansi(buf: &Buffer, color: bool) -> String {
    let mut out = String::new();
    for y in 0..buf.area.height {
        let last = (0..buf.area.width).rev().find(|&x| !buf[(x, y)].symbol().trim().is_empty());
        if let Some(last) = last {
            let mut current: Option<(Color, Modifier)> = None;
            for x in 0..=last {
                let cell = &buf[(x, y)];
                let look = (cell.fg, cell.modifier);
                if color && current != Some(look) {
                    out.push_str(&sgr(look.0, look.1));
                    current = Some(look);
                }
                out.push_str(cell.symbol());
            }
            if color {
                out.push_str("\x1b[0m");
            }
        }
        out.push('\n');
    }
    out
}

/// One SGR sequence that resets, then sets this cell's foreground and boldness.
fn sgr(fg: Color, modifier: Modifier) -> String {
    let mut codes = vec!["0".to_owned()];
    if modifier.contains(Modifier::BOLD) {
        codes.push("1".to_owned());
    }
    let fg = match fg {
        Color::Reset => None,
        Color::Black => Some("30".to_owned()),
        Color::Red => Some("31".to_owned()),
        Color::Green => Some("32".to_owned()),
        Color::Yellow => Some("33".to_owned()),
        Color::Blue => Some("34".to_owned()),
        Color::Magenta => Some("35".to_owned()),
        Color::Cyan => Some("36".to_owned()),
        Color::Gray => Some("37".to_owned()),
        Color::DarkGray => Some("90".to_owned()),
        Color::LightRed => Some("91".to_owned()),
        Color::LightGreen => Some("92".to_owned()),
        Color::LightYellow => Some("93".to_owned()),
        Color::LightBlue => Some("94".to_owned()),
        Color::LightMagenta => Some("95".to_owned()),
        Color::LightCyan => Some("96".to_owned()),
        Color::White => Some("97".to_owned()),
        Color::Indexed(i) => Some(format!("38;5;{i}")),
        Color::Rgb(r, g, b) => Some(format!("38;2;{r};{g};{b}")),
    };
    codes.extend(fg);
    format!("\x1b[{}m", codes.join(";"))
}

fn draw(f: &mut Frame, s: &Snapshot, failing: &[Failing], agents: &[Agent], footer: &str, p: &Palette) {
    let area = f.area().inner(ratatui::layout::Margin { horizontal: 2, vertical: 1 });
    if s.host.name.is_empty() {
        f.render_widget(Paragraph::new("Gathering the first sample…").style(Style::new().fg(p.dim)), area);
        return;
    }
    let o = Overview::new(s);
    let band = Band::new(s, failing.to_vec());

    let mut rows = vec![
        Constraint::Length(1), // host
        Constraint::Length(1), // status
        Constraint::Length(1), // gap
        Constraint::Length(1), // cpu
        Constraint::Length(1), // threads
        Constraint::Length(1), // memory
    ];
    if o.swap.is_some() {
        rows.push(Constraint::Length(1));
    }
    rows.extend(o.disks.iter().map(|_| Constraint::Length(1)));
    rows.extend([
        Constraint::Length(1), // network
        Constraint::Length(1), // gap
        Constraint::Length(if p.quiet { 5 } else { 8 }), // history
        Constraint::Length(1), // gap
    ]);
    // Agents get the rows that are left (heading plus one line each), then the footer.
    let used: u16 = rows.iter().map(|c| if let Constraint::Length(n) = c { *n } else { 0 }).sum();
    let room = area.height.saturating_sub(used + 3);
    let agent_lines = (agents.len() as u16).min(room);
    rows.extend([
        Constraint::Length(1),           // agents heading
        Constraint::Length(agent_lines), // agents
        Constraint::Length(1),           // gap
        Constraint::Length(1),           // footer
    ]);
    let areas = Layout::vertical(rows).split(area);
    let mut next = areas.iter().copied();
    let mut take = || next.next().unwrap_or_default();

    // Who the machine is, and whether anything is wrong.
    let host = take();
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(band.host.clone(), Style::new().add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {} · Linux {}", band.os, band.kernel), Style::new().fg(p.dim)),
        ])),
        host,
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled("up ", Style::new().fg(p.dim)), Span::raw(band.uptime.clone())]))
            .alignment(Alignment::Right),
        host,
    );
    f.render_widget(Paragraph::new(status_line(&band, p)), take());
    take();

    // Vitals: one row each, key colour, gauge, detail.
    let cpu_detail = match &o.temperature {
        Some(t) => format!("{} · {t} · load {}", o.core_summary, o.load_1),
        None => format!("{} · load {}", o.core_summary, o.load_1),
    };
    meter_row(f, take(), p.cpu, "CPU", &o.cpu, &cpu_detail, p);
    threads_row(f, take(), &o.cores, "  per thread    ", p);
    meter_row(f, take(), p.mem, "Memory", &o.memory, &o.memory_text, p);
    if let Some((meter, text)) = &o.swap {
        meter_row(f, take(), p.mem, "Swap", meter, text, p);
    }
    for disk in &o.disks {
        meter_row(f, take(), p.disk, &format!("Disk {}", disk.mount), &disk.meter, &disk.text, p);
    }
    let names = o.networks.iter().map(|n| n.name.as_str()).collect::<Vec<_>>().join(", ");
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("■ ", Style::new().fg(p.net)),
            Span::styled(format!("{:<14}", "Network"), Style::new().add_modifier(Modifier::BOLD)),
            Span::raw(format!("↓ {}   ↑ {}", o.net_rx, o.net_tx)),
            Span::styled(format!("   {names}"), Style::new().fg(p.dim)),
        ])),
        take(),
    );
    take();

    // Two minutes of CPU and memory on one shared 0–100 scale.
    let [cpu_hist, mem_hist] = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).spacing(3).areas(take());
    history(f, cpu_hist, p.cpu, "CPU", &o.cpu.label, &s.cpu_history, p);
    history(f, mem_hist, p.mem, "Memory", &o.memory.label, &s.memory_history, p);
    take();

    agents_section(f, take(), take(), agents, usize::from(agent_lines), p);
    take();
    f.render_widget(Paragraph::new(Span::styled(footer.to_owned(), Style::new().fg(p.dim))), take());
}

/// A heading with the counts, then one line per agent (waiting-for-you first) as far as
/// `lines` allows; the last line says how many didn't fit.
fn agents_section(f: &mut Frame, heading: Rect, list: Rect, agents: &[Agent], lines: usize, p: &Palette) {
    let count = |status: &str| agents.iter().filter(|a| a.status == status).count();
    let mut head = vec![
        Span::raw("■ "),
        Span::styled("Agents", Style::new().add_modifier(Modifier::BOLD)),
        Span::styled(format!("  {} running", agents.len()), Style::new().fg(p.dim)),
    ];
    if count("working") > 0 {
        head.push(Span::raw(format!(" · {} working", count("working"))));
    }
    if count("blocked") > 0 {
        head.push(Span::styled(format!(" · {} waiting for you", count("blocked")), Style::new().fg(p.warn).add_modifier(Modifier::BOLD)));
    }
    f.render_widget(Paragraph::new(Line::from(head)), heading);

    let rows = Layout::vertical((0..lines).map(|_| Constraint::Length(1))).split(list);
    let hidden = agents.len().saturating_sub(lines);
    for (i, (agent, row)) in agents.iter().zip(rows.iter()).enumerate() {
        if hidden > 0 && i + 1 == lines {
            f.render_widget(Paragraph::new(Span::styled(format!("  + {} more", hidden + 1), Style::new().fg(p.dim))), *row);
            break;
        }
        let [state, title, meta] =
            Layout::horizontal([Constraint::Length(18), Constraint::Fill(1), Constraint::Percentage(40)]).spacing(2).areas(*row);
        let state_style = match agent.status.as_str() {
            "blocked" => Style::new().fg(p.warn).add_modifier(Modifier::BOLD),
            "working" => Style::new().add_modifier(Modifier::BOLD),
            "running" => Style::new(),
            _ => Style::new().fg(p.dim),
        };
        f.render_widget(Paragraph::new(Span::styled(format!("  {}", agent_label(&agent.status)), state_style)), state);
        let title_style = if matches!(agent.status.as_str(), "idle" | "done") { Style::new().fg(p.dim) } else { Style::new() };
        f.render_widget(Paragraph::new(Span::styled(agent.title.clone(), title_style)), title);
        let place = agent.workspace.clone().unwrap_or_else(|| "outside herdr".to_owned());
        let load = match (agent.cpu, agent.memory) {
            (Some(cpu), Some(mem)) => format!("{cpu:>5.1}%  {:>8}", crate::views::bytes(mem)),
            _ => String::new(),
        };
        // Where on the left, load in a right-aligned column so the figures line up.
        let [place_area, load_area] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(17)]).areas(meta);
        f.render_widget(Paragraph::new(Span::styled(format!("{} · {place}", agent.kind), Style::new().fg(p.dim))), place_area);
        f.render_widget(Paragraph::new(load).alignment(Alignment::Right), load_area);
    }
}

/// The failing services, or a quiet all-clear.
fn status_line(band: &Band, p: &Palette) -> Line<'static> {
    if band.failing.is_empty() {
        return Line::from(vec![Span::raw("■ "), Span::styled("No failed or crash-looping services", Style::new().fg(p.dim))]);
    }
    let mut spans = vec![
        Span::styled("■ ", Style::new().fg(p.fail)),
        Span::styled(
            format!(
                "{} {} attention: ",
                band.failing.len(),
                if band.failing.len() == 1 { "service needs" } else { "services need" }
            ),
            Style::new().add_modifier(Modifier::BOLD),
        ),
    ];
    for (i, unit) in band.failing.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(", "));
        }
        spans.push(Span::styled(unit.name.clone(), Style::new().fg(p.fail).add_modifier(Modifier::BOLD)));
        let why = if unit.restarting { " crash-looping" } else { " failed" };
        spans.push(Span::styled(why, Style::new().fg(p.dim)));
    }
    Line::from(spans)
}

/// `■ Name   ━━━━━━━━──────  42%   detail [HIGH]`
fn meter_row(f: &mut Frame, area: Rect, key: Color, name: &str, m: &Meter, detail: &str, p: &Palette) {
    let [label, gauge, text] =
        Layout::horizontal([Constraint::Length(14), Constraint::Fill(1), Constraint::Percentage(45)]).spacing(2).areas(area);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("■ ", Style::new().fg(key)),
            Span::styled(name.to_owned(), Style::new().add_modifier(Modifier::BOLD)),
        ])),
        label,
    );
    let fill = match m.level {
        "crit" => p.fail,
        _ => key,
    };
    f.render_widget(
        LineGauge::default()
            .ratio(f64::from(m.percent) / 100.0)
            .label(Span::styled(format!("{:>4}", m.label), level_style(m.level, p)))
            .filled_symbol(symbols::line::THICK_HORIZONTAL)
            .filled_style(Style::new().fg(fill))
            .unfilled_symbol(if p.quiet { " " } else { symbols::line::HORIZONTAL })
            .unfilled_style(Style::new().fg(p.dim)),
        gauge,
    );
    let mut spans = vec![Span::raw(detail.to_owned())];
    match m.level {
        "warn" => spans.push(Span::styled("  HIGH", Style::new().fg(p.warn).add_modifier(Modifier::BOLD))),
        "crit" => spans.push(Span::styled("  CRITICAL", Style::new().fg(p.fail).add_modifier(Modifier::BOLD))),
        _ => {}
    }
    f.render_widget(Paragraph::new(Line::from(spans)), text);
}

/// Per-thread load as one block character each, under the p.cpu row.
fn threads_row(f: &mut Frame, area: Rect, cores: &[Meter], prefix: &str, p: &Palette) {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let mut spans = vec![Span::styled(prefix.to_owned(), Style::new().fg(p.dim))];
    for core in cores {
        let step = (usize::from(core.percent) * (BLOCKS.len() - 1) + 50) / 100;
        let color = if core.level == "crit" { p.fail } else { p.cpu };
        spans.push(Span::styled(BLOCKS[step].to_string(), Style::new().fg(color)));
        spans.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// A titled sparkline of 0–100 values.
fn history(f: &mut Frame, area: Rect, key: Color, name: &str, now: &str, values: &[f32], p: &Palette) {
    let [title, chart] = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("■ ", Style::new().fg(key)),
            Span::styled(format!("{name} {now}"), Style::new().add_modifier(Modifier::BOLD)),
            Span::styled("  last 2 min", Style::new().fg(p.dim)),
        ])),
        title,
    );
    // Newest first, drawn from the right edge, so the history fills in from the right.
    let newest_first: Vec<u64> = values.iter().rev().map(|v| v.clamp(0.0, 100.0).round() as u64).collect();
    f.render_widget(
        Sparkline::default()
            .data(&newest_first)
            .max(100)
            .direction(RenderDirection::RightToLeft)
            .style(Style::new().fg(key)),
        chart,
    );
}

fn level_style(level: &str, p: &Palette) -> Style {
    match level {
        "warn" => Style::new().add_modifier(Modifier::BOLD),
        "crit" => Style::new().fg(p.fail).add_modifier(Modifier::BOLD),
        _ => Style::new(),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::{Color, Modifier};

    use super::sgr;

    #[test]
    fn sgr_codes() {
        assert_eq!(sgr(Color::Reset, Modifier::empty()), "\x1b[0m");
        assert_eq!(sgr(Color::Rgb(1, 2, 3), Modifier::BOLD), "\x1b[0;1;38;2;1;2;3m");
        assert_eq!(sgr(Color::DarkGray, Modifier::empty()), "\x1b[0;90m");
    }
}
