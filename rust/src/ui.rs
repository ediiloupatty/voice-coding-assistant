use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
};

use crate::app::{App, BubbleRole, InputMode, MenuState};

// ── Palet warna (light theme — sesuai tts/src/App.css) ──────────────────────
//
// Gradient web (atas→bawah): #d2f3ec → #e1f6f1 → #edf8f5 → #f4f7fb
//
const ACCENT:     Color = Color::Rgb(13,  148, 136); // #0d9488 web --accent
const MUTED:      Color = Color::Rgb(91,  101, 115); // #5b6573 web --muted
const TOOL_C:     Color = Color::Rgb(37,  99,  235); // #2563eb web blue
const WARNING:    Color = Color::Rgb(234, 88,  12);
const ERROR_C:    Color = Color::Rgb(220, 38,  38);
const BORDER:     Color = Color::Rgb(167, 215, 209); // teal muted
#[allow(dead_code)]
const HEADER_BG:  Color = Color::Rgb(210, 243, 236); // #d2f3ec (tak dipakai — header kini netral)
const STATUS_BG:  Color = Color::Rgb(237, 248, 245); // #edf8f5
const INPUT_BG:   Color = Color::Rgb(255, 255, 255);
const CHAT_BG:    Color = Color::Rgb(244, 247, 251); // #f4f7fb web --page
const TEXT_FG:    Color = Color::Rgb(15,  23,  42);  // #0f172a web --ink
const POPUP_BG:   Color = Color::Rgb(15,  23,  42);  // dark navy untuk menu overlay
const POPUP_SEL:  Color = Color::Rgb(13,  148, 136); // teal untuk selected item
const POPUP_DIM:  Color = Color::Rgb(100, 120, 150); // abu-abu terang pada bg gelap

const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

fn spin(idx: usize) -> char {
    SPINNER_FRAMES[idx % SPINNER_FRAMES.len()]
}

/// Meter "level suara" animasi 4-batang — kesan suara masuk saat hearing you.
fn vu_meter(idx: usize) -> String {
    const BARS: [char; 6] = ['▁', '▂', '▃', '▅', '▆', '▇'];
    (0..4)
        .map(|i| BARS[(idx + i * 2) % BARS.len()])
        .collect()
}

// ── Layout ───────────────────────────────────────────────────────────────────
//
// ┌────────────────────────────────────────────┐  Fill(1)     Chat (mulai atas)
// ├────────────────────────────────────────────┤  Length(3)   Input area (form)
// ├────────────────────────────────────────────┤  Length(1)   Status bar (info)
// └────────────────────────────────────────────┘
// Tanpa header judul atas. Form input diletakkan DI ATAS bar info (model/voice/dll).

pub fn draw(frame: &mut Frame, app: &mut App) {
    // Header judul atas dihapus — chat langsung mulai dari baris paling atas.
    // Identitas (VOCA/model/lang/folder/mode) tetap ada di banner pembuka, dan
    // indikator bahasa pindah ke status bar bawah.
    let chunks = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(frame.area());

    render_chat(frame, app, chunks[0]);
    render_input(frame, app, chunks[1]);
    render_status(frame, app, chunks[2]);

    // Palet slash command mengambang tepat di atas bar input saat user mengetik "/".
    if app.input_mode == InputMode::Normal {
        let matches = crate::app::slash_matches(app.input.value());
        if !matches.is_empty() {
            render_slash_palette(frame, app, &matches, chunks[1]);
        }
    }

    if let Some(ref menu) = app.menu {
        render_menu(frame, menu, frame.area());
    }
}

// ── Chat ─────────────────────────────────────────────────────────────────────

fn render_chat(frame: &mut Frame, app: &mut App, area: Rect) {
    // Lebar konten: sisakan 1 kolom untuk scrollbar + indentasi kiri 2 kolom.
    // Teks di-wrap MANUAL agar baris lanjutan tetap punya indentasi (tak meluber
    // ke tepi kiri frame). Tanpa garis vertikal "│".
    const INDENT: &str = "  ";
    let avail = area.width.saturating_sub(1) as usize;
    let wrap_w = avail.saturating_sub(INDENT.len()).max(1);

    let mut lines: Vec<Line> = Vec::new();

    // Tambah blok teks dengan indentasi, di-wrap manual ke `wrap_w`.
    let push_body = |lines: &mut Vec<Line>, text: &str, style: Style| {
        for row in wrap_text(text, wrap_w) {
            lines.push(Line::from(vec![
                Span::raw(INDENT),
                Span::styled(row, style),
            ]));
        }
    };
    let header = |label: &str| {
        Line::from(vec![
            Span::raw(INDENT),
            Span::styled(label.to_string(), Style::default().fg(ACCENT).bold()),
        ])
    };

    for bubble in &app.messages {
        if !lines.is_empty() { lines.push(Line::raw("")); }

        match bubble.role {
            BubbleRole::System => {
                let style = if bubble.content.starts_with("❌") {
                    Style::default().fg(ERROR_C)
                } else if bubble.content.starts_with("⚠") || bubble.content.starts_with("🔒") {
                    Style::default().fg(WARNING)
                } else {
                    Style::default().fg(MUTED).italic()
                };
                push_body(&mut lines, &bubble.content, style);
            }

            BubbleRole::User => {
                lines.push(header("YOU"));
                push_body(&mut lines, &bubble.content, Style::default().fg(TEXT_FG));
            }

            BubbleRole::Assistant => {
                lines.push(header("VOCA"));
                push_body(&mut lines, &bubble.content, Style::default().fg(TEXT_FG));
            }

            BubbleRole::Tool => {
                push_body(&mut lines, &bubble.content, Style::default().fg(TOOL_C).dim());
            }
        }
    }

    // Streaming: jawaban LLM yang sedang masuk
    if app.is_streaming {
        if !lines.is_empty() { lines.push(Line::raw("")); }
        lines.push(header("VOCA"));

        if app.current_stream.is_empty() {
            let sp = spin(app.spinner_frame);
            let msg = if app.spinner_msg.is_empty() { "thinking..." } else { &app.spinner_msg };
            lines.push(Line::from(vec![
                Span::raw(INDENT),
                Span::styled(format!("{sp} "), Style::default().fg(ACCENT)),
                Span::styled(msg.to_string(), Style::default().fg(MUTED).italic()),
            ]));
        } else {
            push_body(&mut lines, &app.current_stream, Style::default().fg(TEXT_FG));
            // Kursor animasi streaming
            lines.push(Line::from(vec![
                Span::raw(INDENT),
                Span::styled(spin(app.spinner_frame).to_string(), Style::default().fg(ACCENT)),
            ]));
        }
    }

    // Teks sudah di-wrap manual → jumlah baris = lines.len() (tepat).
    app.total_lines = lines.len() as u16;
    app.chat_height = area.height; // dipakai handler scroll untuk batas bawah

    let visible_h = area.height;
    let max_off = app.total_lines.saturating_sub(visible_h);
    if app.is_at_bottom {
        app.scroll_offset = max_off;
    } else {
        // Jaga agar tak pernah scroll melewati pesan terakhir (ke area kosong).
        app.scroll_offset = app.scroll_offset.min(max_off);
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .style(Style::default().bg(CHAT_BG).fg(TEXT_FG))
            .scroll((app.scroll_offset, 0)),
        area,
    );

    // Scrollbar vertikal
    let content_len = app.total_lines as usize;
    let viewport    = visible_h as usize;
    if content_len > viewport {
        let mut state = ScrollbarState::new(content_len.saturating_sub(viewport))
            .position(app.scroll_offset as usize);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"))
                .track_symbol(Some("│"))
                .thumb_symbol("█"),
            area,
            &mut state,
        );
    }
}

/// Wrap teks ke lebar `width` (kolom), pecah per kata; kata yang lebih panjang
/// dari `width` dipotong paksa. Menghormati newline `\n` yang sudah ada.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out: Vec<String> = Vec::new();

    for raw_line in text.split('\n') {
        if raw_line.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut cur = String::new();
        let mut cur_w = 0usize;

        for word in raw_line.split(' ') {
            let wlen = word.chars().count();

            // Kata lebih panjang dari lebar → potong paksa per `width` char.
            if wlen > width {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                let mut chunk = String::new();
                for ch in word.chars() {
                    if chunk.chars().count() == width {
                        out.push(std::mem::take(&mut chunk));
                    }
                    chunk.push(ch);
                }
                cur = chunk;
                cur_w = cur.chars().count();
                continue;
            }

            let needed = if cur.is_empty() { wlen } else { cur_w + 1 + wlen };
            if needed > width {
                out.push(std::mem::take(&mut cur));
                cur = word.to_string();
                cur_w = wlen;
            } else {
                if !cur.is_empty() {
                    cur.push(' ');
                    cur_w += 1;
                }
                cur.push_str(word);
                cur_w += wlen;
            }
        }
        out.push(cur);
    }
    out
}

// ── Status bar ───────────────────────────────────────────────────────────────

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    let model_str = format!(" Model · {} ", app.provider.model);

    let (icon, mode_str, mode_style) = if app.voice.listen {
        ("◉", " VOICE ", Style::default().fg(ACCENT).bg(STATUS_BG).bold())
    } else {
        ("⌨", " TEXT ", Style::default().fg(MUTED).bg(STATUS_BG))
    };

    // Hint singkat. "t = type" sudah tampil di bar input saat listening (tak diulang
    // di sini); daftar command kini lewat palet (ketik "/").
    let hint = if app.voice.listen {
        " speak freely  ·  type / for commands "
    } else {
        " type / for commands "
    };

    let lang_str = format!(" lang · {} ", app.voice.lang.to_uppercase());

    let bar = Line::from(vec![
        Span::styled(model_str,              Style::default().fg(MUTED).bg(STATUS_BG)),
        Span::styled("│",                    Style::default().fg(BORDER).bg(STATUS_BG)),
        Span::styled(format!(" {icon}"),     mode_style),
        Span::styled(mode_str,               mode_style),
        Span::styled("│",                    Style::default().fg(BORDER).bg(STATUS_BG)),
        Span::styled(lang_str,               Style::default().fg(MUTED).bg(STATUS_BG)),
        Span::styled("│",                    Style::default().fg(BORDER).bg(STATUS_BG)),
        Span::styled(hint,                   Style::default().fg(MUTED).bg(STATUS_BG)),
        Span::styled(" ".repeat(area.width as usize), Style::default().bg(STATUS_BG)),
    ]);

    frame.render_widget(
        Paragraph::new(bar).style(Style::default().bg(STATUS_BG)),
        area,
    );
}

// ── Input area ───────────────────────────────────────────────────────────────

fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    // Saat ada popup (Menu) aktif, border input dibuat redup agar fokus ke popup.
    let border_color = if app.input_mode == InputMode::Menu { MUTED } else { ACCENT };

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(INPUT_BG).fg(TEXT_FG));

    let inner = block.inner(area);

    match &app.input_mode {
        InputMode::Normal => {
            let value = app.input.value();
            let placeholder = if app.voice_text_mode {
                "type a message… (Esc = back to listening)"
            } else {
                "type a message or /help…"
            };
            let display = if value.is_empty() {
                Line::from(vec![
                    Span::styled("  › ", Style::default().fg(ACCENT).bold()),
                    Span::styled(placeholder, Style::default().fg(MUTED).italic()),
                ])
            } else {
                Line::from(vec![
                    Span::styled("  › ", Style::default().fg(ACCENT).bold()),
                    Span::styled(value.to_string(), Style::default().fg(TEXT_FG)),
                ])
            };
            frame.render_widget(Paragraph::new(display).block(block), area);
            // Posisi kursor: 4 karakter "  › " + visual cursor
            let cursor_x = area.x + 4 + app.input.visual_cursor() as u16;
            frame.set_cursor_position(Position::new(cursor_x, inner.y));
        }

        InputMode::Listening => {
            let sp = spin(app.spinner_frame);
            // Indikator real-time: saat suara user terdeteksi → meter "level" hijau
            // & teks "hearing you", selain itu status menunggu biasa.
            let (icon, label, label_c) = if app.vad_speech {
                (format!("◉ {} ", vu_meter(app.spinner_frame)), "hearing you…", ACCENT)
            } else {
                (format!("◉ {sp} "), "listening… just speak", MUTED)
            };
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(icon, Style::default().fg(ACCENT).bold()),
                    Span::styled(label, Style::default().fg(label_c).italic()),
                    Span::styled("    ·    ", Style::default().fg(MUTED)),
                    Span::styled("t", Style::default().fg(TEXT_FG).bold()),
                    Span::styled(" = type   ", Style::default().fg(MUTED)),
                    Span::styled("/", Style::default().fg(TEXT_FG).bold()),
                    Span::styled(" = commands", Style::default().fg(MUTED)),
                ]))
                .block(block),
                area,
            );
        }

        InputMode::Processing => {
            let sp  = spin(app.spinner_frame);
            let msg = if app.spinner_msg.is_empty() { "thinking…" } else { &app.spinner_msg };
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{sp} "), Style::default().fg(MUTED)),
                    Span::styled(msg.to_string(), Style::default().fg(MUTED).italic()),
                    Span::styled("   ·  Esc to stop", Style::default().fg(MUTED)),
                ]))
                .block(block),
                area,
            );
        }

        InputMode::Speaking => {
            let sp = spin(app.spinner_frame);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("♪ {sp} "), Style::default().fg(ACCENT).bold()),
                    Span::styled("speaking… (mic off)", Style::default().fg(ACCENT).italic()),
                ]))
                .block(block),
                area,
            );
        }

        InputMode::Menu => {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  ↑/↓ select  ·  Enter confirm  ·  q cancel",
                    Style::default().fg(MUTED).italic(),
                )))
                .block(block),
                area,
            );
        }
    }
}

// ── Menu overlay ─────────────────────────────────────────────────────────────

fn render_menu(frame: &mut Frame, menu: &MenuState, area: Rect) {
    // Warna aksen: merah bila danger (mis. konfirmasi rm -rf), selain itu teal.
    let accent = if menu.danger { ERROR_C } else { POPUP_SEL };

    // Lebar: muat item terpanjang (sudah termasuk prefiks "❯ 1. "), judul, subjudul.
    let prefix_w = 5u16; // "❯ N. "
    let max_item_w = menu.items.iter().map(|s| s.chars().count()).max().unwrap_or(10) as u16 + prefix_w;
    let subtitle_w = menu.subtitle.as_ref().map(|s| s.chars().count()).unwrap_or(0) as u16;
    let content_w  = max_item_w
        .max(menu.title.chars().count() as u16)
        .max(subtitle_w)
        .max(30);
    let popup_w    = (content_w + 4).min(area.width);

    let mut lines: Vec<Line> = Vec::new();

    // Subjudul opsional (mis. path folder untuk trust).
    if let Some(sub) = &menu.subtitle {
        let inner_w = popup_w.saturating_sub(4) as usize;
        let disp = ellipsize(sub, inner_w);
        lines.push(Line::from(Span::styled(disp, Style::default().fg(Color::White).bold())));
        lines.push(Line::raw(""));
    }

    // Item bernomor — nomor hanya keterangan; pemilihan tetap lewat cursor (↑/↓).
    for (i, item) in menu.items.iter().enumerate() {
        let num = format!("{}. ", i + 1);
        if i == menu.selected {
            lines.push(Line::from(vec![
                Span::styled(" ❯ ", Style::default().fg(accent).bold()),
                Span::styled(num, Style::default().fg(accent).bold()),
                Span::styled(item.clone(), Style::default().fg(Color::White).bold()),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(num, Style::default().fg(POPUP_DIM)),
                Span::styled(item.clone(), Style::default().fg(POPUP_DIM)),
            ]));
        }
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        " ↑/↓ select  ·  Enter ok  ·  q cancel",
        Style::default().fg(POPUP_DIM).italic(),
    )));

    let popup_h    = (lines.len() as u16 + 2).min(area.height);
    let popup_area = centered_rect(popup_w, popup_h, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(menu.title.clone(), Style::default().fg(accent).bold()),
            Span::raw(" "),
        ]))
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(POPUP_BG));

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: false }),
        popup_area,
    );
}

// ── Slash command palette ──────────────────────────────────────────────────────

/// Popup mengambang di atas bar input: daftar command + deskripsi. Nomor/posisi
/// dipilih lewat cursor (↑/↓); Tab melengkapi, Enter menjalankan.
fn render_slash_palette(
    frame: &mut Frame,
    app: &App,
    matches: &[&'static crate::app::SlashCmd],
    input_area: Rect,
) {
    let sel = app.slash_sel.min(matches.len().saturating_sub(1));

    let name_w = matches.iter().map(|(n, _, _)| n.chars().count()).max().unwrap_or(6);
    let args_w = matches.iter().map(|(_, a, _)| a.chars().count()).max().unwrap_or(0);
    let desc_w = matches.iter().map(|(_, _, d)| d.chars().count()).max().unwrap_or(0);

    // " ❯ {name}  {args}  {desc} "
    let row_w = 3 + name_w + 2 + args_w + 2 + desc_w + 1;
    let popup_w = (row_w as u16).min(input_area.width);
    let popup_h = (matches.len() as u16 + 2).min(input_area.y); // +2 untuk border
    let x = input_area.x;
    let y = input_area.y.saturating_sub(popup_h);
    let popup_area = Rect::new(x, y, popup_w, popup_h);

    frame.render_widget(Clear, popup_area);

    let mut lines: Vec<Line> = Vec::new();
    for (i, (name, args, desc)) in matches.iter().enumerate() {
        let name_pad = format!("{:<name_w$}", name, name_w = name_w);
        let args_pad = format!("{:<args_w$}", args, args_w = args_w);
        if i == sel {
            lines.push(Line::from(vec![
                Span::styled(" ❯ ", Style::default().fg(POPUP_SEL).bold()),
                Span::styled(name_pad, Style::default().fg(Color::White).bold()),
                Span::styled(format!("  {args_pad}"), Style::default().fg(POPUP_DIM)),
                Span::styled(format!("  {desc}"), Style::default().fg(POPUP_SEL)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(name_pad, Style::default().fg(POPUP_DIM)),
                Span::styled(format!("  {args_pad}"), Style::default().fg(POPUP_DIM)),
                Span::styled(format!("  {desc}"), Style::default().fg(POPUP_DIM)),
            ]));
        }
    }

    let block = Block::default()
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled("commands", Style::default().fg(POPUP_SEL).bold()),
            Span::styled("  ↑/↓ · Tab · Enter", Style::default().fg(POPUP_DIM)),
            Span::raw(" "),
        ]))
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(POPUP_SEL))
        .style(Style::default().bg(POPUP_BG));

    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), popup_area);
}

/// Potong string di depan dengan "…" bila lebih panjang dari `max` kolom.
fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max || max == 0 {
        return s.to_string();
    }
    let tail: String = s.chars().rev().take(max.saturating_sub(1)).collect();
    let tail: String = tail.chars().rev().collect();
    format!("…{tail}")
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
