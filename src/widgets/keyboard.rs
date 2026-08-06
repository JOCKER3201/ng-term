//! On-screen keyboard — a replica of the eDEX-UI keyboard (en-US layout).
//! Clickable: sends sequences to the PTY, with sticky SHIFT/CTRL/ALT/FN modifiers.

use super::{Ctx, Rect};
use crate::font::FONT_UI;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq)]
pub enum Action {
    /// Plain character; the second element is the SHIFT variant.
    Char(char, char),
    /// Fixed byte sequence.
    Seq(&'static [u8]),
    Shift,
    Ctrl,
    Alt,
    Fn,
}

pub struct KeyDef {
    pub label: &'static str,
    pub shift_label: &'static str,
    pub w: f32,
    pub action: Action,
}

const fn ch(label: &'static str, c: char, shift_label: &'static str, s: char) -> KeyDef {
    KeyDef { label, shift_label, w: 1.0, action: Action::Char(c, s) }
}

const fn letter(label: &'static str, c: char, s: char) -> KeyDef {
    KeyDef { label, shift_label: "", w: 1.0, action: Action::Char(c, s) }
}

const fn seq(label: &'static str, w: f32, bytes: &'static [u8]) -> KeyDef {
    KeyDef { label, shift_label: "", w, action: Action::Seq(bytes) }
}

const fn modk(label: &'static str, w: f32, action: Action) -> KeyDef {
    KeyDef { label, shift_label: "", w, action }
}

pub fn layout() -> [Vec<KeyDef>; 5] {
    [
        vec![
            seq("ESC", 1.3, b"\x1b"),
            ch("`", '`', "~", '~'),
            ch("1", '1', "!", '!'),
            ch("2", '2', "@", '@'),
            ch("3", '3', "#", '#'),
            ch("4", '4', "$", '$'),
            ch("5", '5', "%", '%'),
            ch("6", '6', "^", '^'),
            ch("7", '7', "&", '&'),
            ch("8", '8', "*", '*'),
            ch("9", '9', "(", '('),
            ch("0", '0', ")", ')'),
            ch("-", '-', "_", '_'),
            ch("=", '=', "+", '+'),
            seq("BACK", 1.8, b"\x7f"),
        ],
        vec![
            seq("TAB", 1.6, b"\t"),
            letter("Q", 'q', 'Q'),
            letter("W", 'w', 'W'),
            letter("E", 'e', 'E'),
            letter("R", 'r', 'R'),
            letter("T", 't', 'T'),
            letter("Y", 'y', 'Y'),
            letter("U", 'u', 'U'),
            letter("I", 'i', 'I'),
            letter("O", 'o', 'O'),
            letter("P", 'p', 'P'),
            ch("[", '[', "{", '{'),
            ch("]", ']', "}", '}'),
            ch("\\", '\\', "|", '|'),
        ],
        vec![
            modk("FN", 1.9, Action::Fn),
            letter("A", 'a', 'A'),
            letter("S", 's', 'S'),
            letter("D", 'd', 'D'),
            letter("F", 'f', 'F'),
            letter("G", 'g', 'G'),
            letter("H", 'h', 'H'),
            letter("J", 'j', 'J'),
            letter("K", 'k', 'K'),
            letter("L", 'l', 'L'),
            ch(";", ';', ":", ':'),
            ch("'", '\'', "\"", '"'),
            seq("ENTER", 2.0, b"\r"),
        ],
        vec![
            modk("SHIFT", 2.4, Action::Shift),
            letter("Z", 'z', 'Z'),
            letter("X", 'x', 'X'),
            letter("C", 'c', 'C'),
            letter("V", 'v', 'V'),
            letter("B", 'b', 'B'),
            letter("N", 'n', 'N'),
            letter("M", 'm', 'M'),
            ch(",", ',', "<", '<'),
            ch(".", '.', ">", '>'),
            ch("/", '/', "?", '?'),
            modk("SHIFT", 2.4, Action::Shift),
        ],
        vec![
            modk("CTRL", 1.6, Action::Ctrl),
            modk("ALT", 1.4, Action::Alt),
            seq("SPACE", 8.0, b" "),
            modk("ALT", 1.2, Action::Alt),
            seq("\u{2190}", 1.0, b"\x1b[D"),
            seq("\u{2193}", 1.0, b"\x1b[B"),
            seq("\u{2191}", 1.0, b"\x1b[A"),
            seq("\u{2192}", 1.0, b"\x1b[C"),
        ],
    ]
}

pub struct Keyboard {
    rows: [Vec<KeyDef>; 5],
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub fn_mod: bool,
    /// Key rectangles from the last frame: (rect, row, column).
    hits: Vec<(Rect, usize, usize)>,
    /// Time of the last press (highlight) per key.
    pressed: std::collections::HashMap<(usize, usize), Instant>,
}

impl Keyboard {
    pub fn new() -> Self {
        Keyboard {
            rows: layout(),
            shift: false,
            ctrl: false,
            alt: false,
            fn_mod: false,
            hits: Vec::new(),
            pressed: std::collections::HashMap::new(),
        }
    }

    /// Highlight the key matching a character from the physical keyboard.
    pub fn flash_char(&mut self, c: char) {
        let lc = c.to_ascii_lowercase();
        for (ri, row) in self.rows.iter().enumerate() {
            for (ki, key) in row.iter().enumerate() {
                if let Action::Char(base, shifted) = key.action {
                    if base == lc || shifted == c {
                        self.pressed.insert((ri, ki), Instant::now());
                        return;
                    }
                }
            }
        }
    }

    pub fn flash_label(&mut self, label: &str) {
        for (ri, row) in self.rows.iter().enumerate() {
            for (ki, key) in row.iter().enumerate() {
                if key.label == label {
                    self.pressed.insert((ri, ki), Instant::now());
                    return;
                }
            }
        }
    }

    /// Click handling; returns bytes to send to the PTY.
    pub fn click(&mut self, x: f32, y: f32) -> Option<Vec<u8>> {
        let hit = self
            .hits
            .iter()
            .find(|(r, _, _)| r.contains(x, y))
            .map(|&(_, ri, ki)| (ri, ki))?;
        let (ri, ki) = hit;
        self.pressed.insert((ri, ki), Instant::now());
        let action = self.rows[ri][ki].action;
        match action {
            Action::Shift => {
                self.shift = !self.shift;
                None
            }
            Action::Ctrl => {
                self.ctrl = !self.ctrl;
                None
            }
            Action::Alt => {
                self.alt = !self.alt;
                None
            }
            Action::Fn => {
                self.fn_mod = !self.fn_mod;
                None
            }
            Action::Char(base, shifted) => {
                let mut out = Vec::new();
                // FN + digit = function key (like eDEX).
                if self.fn_mod {
                    if let Some(fseq) = fn_seq(base) {
                        out.extend_from_slice(fseq);
                        self.clear_sticky();
                        return Some(out);
                    }
                }
                let c = if self.shift { shifted } else { base };
                if self.ctrl {
                    let lc = base.to_ascii_lowercase();
                    if lc.is_ascii_alphabetic() {
                        out.push((lc as u8) & 0x1f);
                    } else if "[\\]^_@ ".contains(lc) {
                        out.push((lc as u8) & 0x1f);
                    } else {
                        out.extend_from_slice(c.to_string().as_bytes());
                    }
                } else {
                    if self.alt {
                        out.push(0x1b);
                    }
                    out.extend_from_slice(c.to_string().as_bytes());
                }
                self.clear_sticky();
                Some(out)
            }
            Action::Seq(s) => {
                let mut out = Vec::new();
                if self.alt {
                    out.push(0x1b);
                }
                out.extend_from_slice(s);
                self.clear_sticky();
                Some(out)
            }
        }
    }

    fn clear_sticky(&mut self) {
        self.shift = false;
        self.ctrl = false;
        self.alt = false;
        self.fn_mod = false;
    }

    pub fn draw(&mut self, ctx: &mut Ctx, r: Rect) {
        self.hits.clear();
        let base = ctx.theme.base;
        let n_rows = self.rows.len();
        let gap = ctx.vh(0.46);
        let key_h = (r.h - gap * (n_rows as f32 + 1.0)) / n_rows as f32;
        let px = ctx.font_px(1.05);
        let spx = ctx.font_px(0.75);
        let now = Instant::now();

        for (ri, row) in self.rows.iter().enumerate() {
            let total_units: f32 = row.iter().map(|k| k.w).sum::<f32>();
            let unit = (r.w - gap * (row.len() as f32 - 1.0)) / total_units;
            let mut x = r.x;
            let y = r.y + gap + (key_h + gap) * ri as f32;
            for (ki, key) in row.iter().enumerate() {
                let kw = unit * key.w;
                let krect = Rect::new(x, y, kw, key_h);

                // Key state: recently pressed or an active sticky modifier.
                let flash = self
                    .pressed
                    .get(&(ri, ki))
                    .map(|t| now.duration_since(*t).as_secs_f32() < 0.15)
                    .unwrap_or(false);
                let sticky = matches!(
                    (key.action, self.shift, self.ctrl, self.alt, self.fn_mod),
                    (Action::Shift, true, _, _, _)
                        | (Action::Ctrl, _, true, _, _)
                        | (Action::Alt, _, _, true, _)
                        | (Action::Fn, _, _, _, true)
                );
                if flash || sticky {
                    ctx.dl.rect(krect.x, krect.y, krect.w, krect.h, base.alpha(0.35));
                } else {
                    ctx.dl.rect(krect.x, krect.y, krect.w, krect.h, ctx.theme.bg);
                }
                ctx.dl
                    .rect_outline(krect.x, krect.y, krect.w, krect.h, 1.0, base.alpha(0.3));

                // Main label in the center. Arrows are drawn as vectors
                // because the UI font may lack those glyphs.
                let label = if self.fn_mod {
                    if let Action::Char(b, _) = key.action {
                        fn_label(b).unwrap_or(key.label)
                    } else {
                        key.label
                    }
                } else {
                    key.label
                };
                if let Some(dir) = arrow_dir(label) {
                    let cx = krect.cx();
                    let cy = y + key_h / 2.0;
                    let s = (key_h * 0.16).max(4.0);
                    let (a, b, c) = match dir {
                        0 => ([cx - s, cy], [cx + s, cy - s], [cx + s, cy + s]), // ←
                        1 => ([cx, cy - s], [cx - s, cy + s], [cx + s, cy + s]), // ↑
                        2 => ([cx + s, cy], [cx - s, cy - s], [cx - s, cy + s]), // →
                        _ => ([cx, cy + s], [cx - s, cy - s], [cx + s, cy - s]), // ↓
                    };
                    ctx.dl.quad([a, b, c, c], base);
                } else {
                    ctx.dl.text_center(
                        ctx.fonts,
                        FONT_UI,
                        px,
                        krect.cx(),
                        y + (key_h - px * 1.3) / 2.0,
                        label,
                        base,
                        px * 0.05,
                    );
                }
                // SHIFT variant in the top-right corner.
                if !key.shift_label.is_empty() {
                    ctx.dl.text_right(
                        ctx.fonts,
                        FONT_UI,
                        spx,
                        krect.right() - spx * 0.4,
                        y + spx * 0.3,
                        key.shift_label,
                        base.alpha(0.5),
                        0.0,
                    );
                }

                self.hits.push((krect, ri, ki));
                x += kw + gap;
            }
        }
    }
}

/// 0 = left, 1 = up, 2 = right, 3 = down.
fn arrow_dir(label: &str) -> Option<u8> {
    match label {
        "\u{2190}" => Some(0),
        "\u{2191}" => Some(1),
        "\u{2192}" => Some(2),
        "\u{2193}" => Some(3),
        _ => None,
    }
}

fn fn_seq(digit: char) -> Option<&'static [u8]> {
    Some(match digit {
        '1' => b"\x1bOP",
        '2' => b"\x1bOQ",
        '3' => b"\x1bOR",
        '4' => b"\x1bOS",
        '5' => b"\x1b[15~",
        '6' => b"\x1b[17~",
        '7' => b"\x1b[18~",
        '8' => b"\x1b[19~",
        '9' => b"\x1b[20~",
        '0' => b"\x1b[21~",
        '-' => b"\x1b[23~",
        '=' => b"\x1b[24~",
        _ => return None,
    })
}

fn fn_label(digit: char) -> Option<&'static str> {
    Some(match digit {
        '1' => "F1",
        '2' => "F2",
        '3' => "F3",
        '4' => "F4",
        '5' => "F5",
        '6' => "F6",
        '7' => "F7",
        '8' => "F8",
        '9' => "F9",
        '0' => "F10",
        '-' => "F11",
        '=' => "F12",
        _ => return None,
    })
}
