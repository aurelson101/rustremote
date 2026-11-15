//! Bounded ANSI screen proof. No OSC callbacks, clipboard access or URL actions.
use crate::probe_output::MAX_OUTPUT;
use std::num::NonZeroU16;

pub struct Frame {
    pub markup: String,
    pub plain: String,
}

fn color(value: vt100::Color, foreground: bool) -> String {
    let rgb = match value {
        vt100::Color::Default => return if foreground { "#e5e9f0" } else { "#151b24" }.into(),
        vt100::Color::Rgb(r, g, b) => [r, g, b],
        vt100::Color::Idx(index) => {
            const BASE: [[u8; 3]; 16] = [
                [21, 27, 36],
                [205, 76, 86],
                [116, 190, 113],
                [224, 193, 108],
                [100, 155, 225],
                [184, 125, 208],
                [96, 190, 198],
                [210, 216, 225],
                [99, 110, 128],
                [242, 111, 120],
                [153, 220, 147],
                [247, 221, 144],
                [139, 185, 250],
                [214, 167, 238],
                [145, 223, 229],
                [245, 247, 250],
            ];
            match index {
                0..=15 => BASE[index as usize],
                16..=231 => {
                    let n = index - 16;
                    let level = |n: u8| if n == 0 { 0 } else { 55 + n * 40 };
                    [level(n / 36), level((n / 6) % 6), level(n % 6)]
                }
                _ => [8 + (index - 232) * 10; 3],
            }
        }
    };
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

pub fn render(bytes: &[u8], rows: u16, cols: u16) -> Result<Frame, String> {
    if bytes.len() > MAX_OUTPUT || !(1..=60).contains(&rows) || !(1..=160).contains(&cols) {
        return Err("ANSI preview exceeds its screen/output budget".into());
    }
    let mut parser = vt100::Parser::new(
        NonZeroU16::new(rows).unwrap(),
        NonZeroU16::new(cols).unwrap(),
        0,
    );
    // Chunking deliberately exercises incremental ANSI and UTF-8 parsing.
    for chunk in bytes.chunks(4096) {
        parser.process(chunk);
    }
    frame(parser.screen())
}

type Style = (vt100::Color, vt100::Color, bool, bool, bool, bool);

fn append_run(markup: &mut String, text: &str, style: Style) {
    use std::fmt::Write;
    let (fg, bg, bold, italic, underline, inverse) = style;
    let (mut fg, mut bg) = (color(fg, true), color(bg, false));
    if inverse {
        std::mem::swap(&mut fg, &mut bg);
    }
    // Group adjacent cells before Pango styling: ordinary output needs one span
    // per row, not thousands of independent attributes on each GUI refresh.
    write!(markup,
        "<span foreground=\"{fg}\" background=\"{bg}\" weight=\"{}\" style=\"{}\" underline=\"{}\">{}</span>",
        if bold { "bold" } else { "normal" },
        if italic { "italic" } else { "normal" },
        if underline { "single" } else { "none" },
        gtk::glib::markup_escape_text(text),
    ).expect("writing to a String cannot fail");
}

pub fn frame(screen: &vt100::Screen) -> Result<Frame, String> {
    let mut markup = String::from("<tt>");
    let (rows, cols) = screen.size();
    let (rows, cols) = (rows.get(), cols.get());
    for row in 0..rows {
        let mut run = String::new();
        let mut previous = None;
        for col in 0..cols {
            let cell = screen.cell(row, col).ok_or("ANSI cell outside screen")?;
            if cell.is_wide_continuation() {
                continue;
            }
            let cursor = !screen.hide_cursor() && screen.cursor_position() == (row, col);
            let style = (
                cell.fgcolor(),
                cell.bgcolor(),
                cell.bold(),
                cell.italic(),
                cell.underline() || cursor,
                cell.inverse(),
            );
            if let Some(old) = previous
                && old != style
            {
                append_run(&mut markup, &run, old);
                run.clear();
            }
            previous = Some(style);
            run.push_str(if cell.has_contents() {
                cell.contents()
            } else {
                " "
            });
        }
        if let Some(style) = previous {
            append_run(&mut markup, &run, style);
        }
        if row + 1 != rows {
            markup.push('\n');
        }
    }
    markup.push_str("</tt>");
    Ok(Frame {
        markup,
        plain: screen.contents(),
    })
}

pub fn preview() -> Frame {
    render(b"\x1b[2J\x1b[H\x1b[1;36mRustRemote ANSI preview\x1b[0m\r\nLocal rendering only - no shell or network.\r\n\x1b[32mUTF-8: \xc3\xa9t\xc3\xa9\x1b[0m\r\nProgress: 10%\r\x1b[2KProgress: 100%\r\n\x1b[38;2;160;190;255mTrue color\x1b[0m | \x1b[4munderline\x1b[0m\r\n", 12, 80)
        .expect("fixed preview fits the screen budget")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interprets_cursor_erase_styles_and_escapes_untrusted_text() {
        let frame = render(b"wrong\r\x1b[2K\x1b[31m<&>\x1b[0m", 2, 10).unwrap();
        assert_eq!(frame.plain, "<&>");
        assert!(frame.markup.contains("&lt;"));
        assert!(!frame.markup.contains("wrong"));
        assert!(frame.markup.contains("#cd4c56"));
    }

    #[test]
    fn parses_split_unicode_wide_cells_and_ignores_osc_actions() {
        let mut parser =
            vt100::Parser::new(NonZeroU16::new(3).unwrap(), NonZeroU16::new(20).unwrap(), 0);
        for byte in
            "été 界e\u{301}\x1b]52;c;dGVzdA==\x07\x1b]8;;https://example.invalid\x07X\x1b]8;;\x07"
                .as_bytes()
        {
            parser.process(&[*byte]);
        }
        let frame = frame(parser.screen()).unwrap();
        assert_eq!(frame.plain, "été 界e\u{301}X");
        assert!(!frame.markup.contains("https:"));
        assert!(parser.screen().cell(0, 5).unwrap().is_wide_continuation());
    }

    #[test]
    fn long_osc_cannot_grow_a_streaming_transcript_or_execute_an_action() {
        let mut parser = vt100::Parser::default();
        parser.process(b"\x1b]52;c;");
        for _ in 0..256 {
            parser.process(&[b'a'; 4096]);
        }
        parser.process(b"\x07safe");
        assert_eq!(parser.screen().contents(), "safe");
    }

    #[test]
    fn bounds_output_and_screen_and_restores_alternate_screen() {
        assert!(render(&[0; MAX_OUTPUT + 1], 24, 80).is_err());
        assert!(render(b"", 0, 80).is_err());
        assert!(render(b"", 60, 161).is_err());
        assert_eq!(
            render(b"main\x1b[?1049halt\x1b[?1049l", 3, 20)
                .unwrap()
                .plain,
            "main"
        );
        assert_eq!(color(vt100::Color::Idx(255), true), "#eeeeee");
        assert!(preview().plain.contains("Progress: 100%"));
    }
}
