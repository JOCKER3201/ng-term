//! Terminal emulation: character grid + VT sequence handling (parser: vte).

use std::collections::VecDeque;
use unicode_width::UnicodeWidthChar;

pub const FLAG_BOLD: u8 = 1;
pub const FLAG_UNDERLINE: u8 = 2;
pub const FLAG_INVERSE: u8 = 4;
pub const FLAG_DIM: u8 = 8;
/// Second cell of a double-width character.
pub const FLAG_WIDE_SPACER: u8 = 16;

#[derive(Clone, Copy, PartialEq)]
pub enum CellColor {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy)]
pub struct Cell {
    pub ch: char,
    pub fg: CellColor,
    pub bg: CellColor,
    pub flags: u8,
}

impl Cell {
    fn blank(bg: CellColor) -> Self {
        Cell {
            ch: ' ',
            fg: CellColor::Default,
            bg,
            flags: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct Pen {
    fg: CellColor,
    bg: CellColor,
    flags: u8,
}

impl Pen {
    fn default() -> Self {
        Pen {
            fg: CellColor::Default,
            bg: CellColor::Default,
            flags: 0,
        }
    }
}

pub struct Term {
    pub cols: usize,
    pub rows: usize,
    screen: Vec<Vec<Cell>>,
    alt_screen: Vec<Vec<Cell>>,
    pub scrollback: VecDeque<Vec<Cell>>,
    pub cur_x: usize,
    pub cur_y: usize,
    saved_cursor: (usize, usize, Pen),
    pen: Pen,
    scroll_top: usize,
    scroll_bottom: usize,
    pub alt_active: bool,
    pub cursor_visible: bool,
    pub app_cursor: bool,
    wrap_pending: bool,
    autowrap: bool,
    /// View scrolled up (number of scrollback lines).
    pub view_offset: usize,
    /// Responses to send to the PTY (DA, CPR etc.).
    pub responses: Vec<u8>,
    /// Title set via OSC 0/2.
    pub title: String,
}

impl Term {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cols = cols.max(2);
        let rows = rows.max(2);
        Term {
            cols,
            rows,
            screen: vec![vec![Cell::blank(CellColor::Default); cols]; rows],
            alt_screen: vec![vec![Cell::blank(CellColor::Default); cols]; rows],
            scrollback: VecDeque::new(),
            cur_x: 0,
            cur_y: 0,
            saved_cursor: (0, 0, Pen::default()),
            pen: Pen::default(),
            scroll_top: 0,
            scroll_bottom: rows - 1,
            alt_active: false,
            cursor_visible: true,
            app_cursor: false,
            wrap_pending: false,
            autowrap: true,
            view_offset: 0,
            responses: Vec::new(),
            title: String::new(),
        }
    }

    pub fn grid(&self) -> &Vec<Vec<Cell>> {
        if self.alt_active { &self.alt_screen } else { &self.screen }
    }

    fn grid_mut(&mut self) -> &mut Vec<Vec<Cell>> {
        if self.alt_active { &mut self.alt_screen } else { &mut self.screen }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(2);
        let rows = rows.max(2);
        if cols == self.cols && rows == self.rows {
            return;
        }
        for grid in [&mut self.screen, &mut self.alt_screen] {
            for row in grid.iter_mut() {
                row.resize(cols, Cell::blank(CellColor::Default));
            }
            while grid.len() < rows {
                grid.push(vec![Cell::blank(CellColor::Default); cols]);
            }
            while grid.len() > rows {
                grid.pop();
            }
        }
        self.cols = cols;
        self.rows = rows;
        self.scroll_top = 0;
        self.scroll_bottom = rows - 1;
        self.cur_x = self.cur_x.min(cols - 1);
        self.cur_y = self.cur_y.min(rows - 1);
        self.wrap_pending = false;
    }

    pub fn scroll_view(&mut self, delta: i32) {
        if delta > 0 {
            self.view_offset =
                (self.view_offset + delta as usize).min(self.scrollback.len());
        } else {
            self.view_offset = self.view_offset.saturating_sub((-delta) as usize);
        }
    }

    /// Line visible on screen, accounting for scrolling.
    pub fn view_row(&self, y: usize) -> Option<&Vec<Cell>> {
        if self.view_offset == 0 || self.alt_active {
            return self.grid().get(y);
        }
        let sb = self.scrollback.len();
        let start = sb - self.view_offset;
        if start + y < sb {
            self.scrollback.get(start + y)
        } else {
            self.grid().get(start + y - sb)
        }
    }

    fn scroll_up(&mut self, n: usize) {
        for _ in 0..n {
            let top = self.scroll_top;
            let bottom = self.scroll_bottom;
            let bg = self.pen.bg;
            let cols = self.cols;
            let alt = self.alt_active;
            let removed = self.grid_mut()[top].clone();
            if !alt && top == 0 {
                self.scrollback.push_back(removed);
                if self.scrollback.len() > 5000 {
                    self.scrollback.pop_front();
                }
            }
            let grid = self.grid_mut();
            for y in top..bottom {
                grid[y] = grid[y + 1].clone();
            }
            grid[bottom] = vec![Cell::blank(bg); cols];
        }
    }

    fn scroll_down(&mut self, n: usize) {
        for _ in 0..n {
            let top = self.scroll_top;
            let bottom = self.scroll_bottom;
            let bg = self.pen.bg;
            let cols = self.cols;
            let grid = self.grid_mut();
            for y in (top + 1..=bottom).rev() {
                grid[y] = grid[y - 1].clone();
            }
            grid[top] = vec![Cell::blank(bg); cols];
        }
    }

    fn linefeed(&mut self) {
        if self.cur_y == self.scroll_bottom {
            self.scroll_up(1);
        } else if self.cur_y + 1 < self.rows {
            self.cur_y += 1;
        }
        self.wrap_pending = false;
    }

    fn put_char(&mut self, c: char) {
        let width = c.width().unwrap_or(1);
        if width == 0 {
            return; // combining characters skipped (simplification)
        }
        if self.wrap_pending && self.autowrap {
            self.cur_x = 0;
            self.linefeed();
        }
        self.wrap_pending = false;
        if self.cur_x + width > self.cols {
            if self.autowrap {
                self.cur_x = 0;
                self.linefeed();
            } else {
                self.cur_x = self.cols - width;
            }
        }
        let (x, y) = (self.cur_x, self.cur_y);
        let pen = self.pen;
        let cols = self.cols;
        let grid = self.grid_mut();
        grid[y][x] = Cell {
            ch: c,
            fg: pen.fg,
            bg: pen.bg,
            flags: pen.flags,
        };
        if width == 2 && x + 1 < cols {
            grid[y][x + 1] = Cell {
                ch: ' ',
                fg: pen.fg,
                bg: pen.bg,
                flags: pen.flags | FLAG_WIDE_SPACER,
            };
        }
        self.cur_x += width;
        if self.cur_x >= self.cols {
            self.cur_x = self.cols - 1;
            self.wrap_pending = true;
        }
    }

    fn erase_line(&mut self, mode: u16) {
        let (x, y) = (self.cur_x, self.cur_y);
        let bg = self.pen.bg;
        let cols = self.cols;
        let grid = self.grid_mut();
        let range = match mode {
            0 => x..cols,
            1 => 0..(x + 1).min(cols),
            _ => 0..cols,
        };
        for i in range {
            grid[y][i] = Cell::blank(bg);
        }
    }

    fn erase_display(&mut self, mode: u16) {
        let (x, y) = (self.cur_x, self.cur_y);
        let bg = self.pen.bg;
        let cols = self.cols;
        let rows = self.rows;
        match mode {
            0 => {
                self.erase_line(0);
                let grid = self.grid_mut();
                for r in y + 1..rows {
                    grid[r] = vec![Cell::blank(bg); cols];
                }
            }
            1 => {
                self.erase_line(1);
                let grid = self.grid_mut();
                for r in 0..y {
                    grid[r] = vec![Cell::blank(bg); cols];
                }
            }
            3 => {
                self.scrollback.clear();
                let grid = self.grid_mut();
                for r in 0..rows {
                    grid[r] = vec![Cell::blank(bg); cols];
                }
            }
            _ => {
                let grid = self.grid_mut();
                for r in 0..rows {
                    grid[r] = vec![Cell::blank(bg); cols];
                }
            }
        }
        let _ = x;
    }

    fn sgr(&mut self, params: &[u16]) {
        let mut i = 0;
        if params.is_empty() {
            self.pen = Pen::default();
            return;
        }
        while i < params.len() {
            let p = params[i];
            match p {
                0 => self.pen = Pen::default(),
                1 => self.pen.flags |= FLAG_BOLD,
                2 => self.pen.flags |= FLAG_DIM,
                4 => self.pen.flags |= FLAG_UNDERLINE,
                7 => self.pen.flags |= FLAG_INVERSE,
                22 => self.pen.flags &= !(FLAG_BOLD | FLAG_DIM),
                24 => self.pen.flags &= !FLAG_UNDERLINE,
                27 => self.pen.flags &= !FLAG_INVERSE,
                30..=37 => self.pen.fg = CellColor::Indexed((p - 30) as u8),
                39 => self.pen.fg = CellColor::Default,
                40..=47 => self.pen.bg = CellColor::Indexed((p - 40) as u8),
                49 => self.pen.bg = CellColor::Default,
                90..=97 => self.pen.fg = CellColor::Indexed((p - 90 + 8) as u8),
                100..=107 => self.pen.bg = CellColor::Indexed((p - 100 + 8) as u8),
                38 | 48 => {
                    let target_fg = p == 38;
                    if i + 1 < params.len() && params[i + 1] == 5 && i + 2 < params.len() {
                        let c = CellColor::Indexed(params[i + 2] as u8);
                        if target_fg { self.pen.fg = c } else { self.pen.bg = c }
                        i += 2;
                    } else if i + 1 < params.len() && params[i + 1] == 2 && i + 4 < params.len() {
                        let c = CellColor::Rgb(
                            params[i + 2] as u8,
                            params[i + 3] as u8,
                            params[i + 4] as u8,
                        );
                        if target_fg { self.pen.fg = c } else { self.pen.bg = c }
                        i += 4;
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn set_mode(&mut self, private: bool, param: u16, enable: bool) {
        if !private {
            return;
        }
        match param {
            1 => self.app_cursor = enable,
            7 => self.autowrap = enable,
            25 => self.cursor_visible = enable,
            47 | 1047 | 1049 => {
                if enable && !self.alt_active {
                    self.alt_active = true;
                    let bg = self.pen.bg;
                    let cols = self.cols;
                    for row in self.alt_screen.iter_mut() {
                        *row = vec![Cell::blank(bg); cols];
                    }
                    if param == 1049 {
                        self.saved_cursor = (self.cur_x, self.cur_y, self.pen);
                        self.cur_x = 0;
                        self.cur_y = 0;
                    }
                } else if !enable && self.alt_active {
                    self.alt_active = false;
                    if param == 1049 {
                        let (x, y, pen) = self.saved_cursor;
                        self.cur_x = x.min(self.cols - 1);
                        self.cur_y = y.min(self.rows - 1);
                        self.pen = pen;
                    }
                }
                self.view_offset = 0;
            }
            _ => {}
        }
    }
}

/// Executor of vte parser events.
pub struct Performer<'a> {
    pub term: &'a mut Term,
}

fn param_or(params: &vte::Params, idx: usize, def: u16) -> u16 {
    params
        .iter()
        .nth(idx)
        .and_then(|p| p.first().copied())
        .filter(|&v| v != 0)
        .unwrap_or(def)
}

fn flat_params(params: &vte::Params) -> Vec<u16> {
    let mut out = Vec::new();
    for sub in params.iter() {
        for &v in sub {
            out.push(v);
        }
    }
    out
}

impl<'a> vte::Perform for Performer<'a> {
    fn print(&mut self, c: char) {
        self.term.put_char(c);
        self.term.view_offset = 0;
    }

    fn execute(&mut self, byte: u8) {
        let t = &mut self.term;
        match byte {
            0x08 => {
                t.cur_x = t.cur_x.saturating_sub(1);
                t.wrap_pending = false;
            }
            0x09 => {
                let next = (t.cur_x / 8 + 1) * 8;
                t.cur_x = next.min(t.cols - 1);
            }
            0x0A | 0x0B | 0x0C => t.linefeed(),
            0x0D => {
                t.cur_x = 0;
                t.wrap_pending = false;
            }
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let t = &mut self.term;
        let private = intermediates.contains(&b'?');
        let p0 = param_or(params, 0, 1) as usize;
        match action {
            'A' => t.cur_y = t.cur_y.saturating_sub(p0).max(0),
            'B' | 'e' => t.cur_y = (t.cur_y + p0).min(t.rows - 1),
            'C' | 'a' => t.cur_x = (t.cur_x + p0).min(t.cols - 1),
            'D' => t.cur_x = t.cur_x.saturating_sub(p0),
            'E' => {
                t.cur_y = (t.cur_y + p0).min(t.rows - 1);
                t.cur_x = 0;
            }
            'F' => {
                t.cur_y = t.cur_y.saturating_sub(p0);
                t.cur_x = 0;
            }
            'G' | '`' => t.cur_x = (p0 - 1).min(t.cols - 1),
            'H' | 'f' => {
                let row = param_or(params, 0, 1) as usize;
                let col = param_or(params, 1, 1) as usize;
                t.cur_y = (row - 1).min(t.rows - 1);
                t.cur_x = (col - 1).min(t.cols - 1);
                t.wrap_pending = false;
            }
            'd' => t.cur_y = (p0 - 1).min(t.rows - 1),
            'J' => {
                let mode = param_or(params, 0, 0);
                let mode = if params.iter().next().is_none() { 0 } else { mode };
                t.erase_display(mode);
            }
            'K' => {
                let mode = params
                    .iter()
                    .next()
                    .and_then(|p| p.first().copied())
                    .unwrap_or(0);
                t.erase_line(mode);
            }
            'L' => {
                if t.cur_y >= t.scroll_top && t.cur_y <= t.scroll_bottom {
                    let save_top = t.scroll_top;
                    t.scroll_top = t.cur_y;
                    t.scroll_down(p0);
                    t.scroll_top = save_top;
                }
            }
            'M' => {
                if t.cur_y >= t.scroll_top && t.cur_y <= t.scroll_bottom {
                    let save_top = t.scroll_top;
                    t.scroll_top = t.cur_y;
                    t.scroll_up(p0);
                    t.scroll_top = save_top;
                }
            }
            'P' => {
                // DCH — delete characters
                let (x, y) = (t.cur_x, t.cur_y);
                let bg = t.pen.bg;
                let cols = t.cols;
                let n = p0.min(cols - x);
                let grid = t.grid_mut();
                grid[y].drain(x..x + n);
                grid[y].extend(std::iter::repeat(Cell::blank(bg)).take(n));
            }
            '@' => {
                // ICH — insert blank characters
                let (x, y) = (t.cur_x, t.cur_y);
                let bg = t.pen.bg;
                let cols = t.cols;
                let n = p0.min(cols - x);
                let grid = t.grid_mut();
                for _ in 0..n {
                    grid[y].insert(x, Cell::blank(bg));
                }
                grid[y].truncate(cols);
            }
            'X' => {
                // ECH — erase n characters
                let (x, y) = (t.cur_x, t.cur_y);
                let bg = t.pen.bg;
                let cols = t.cols;
                let n = p0.min(cols - x);
                let grid = t.grid_mut();
                for i in 0..n {
                    grid[y][x + i] = Cell::blank(bg);
                }
            }
            'S' => t.scroll_up(p0),
            'T' => t.scroll_down(p0),
            'r' => {
                let top = param_or(params, 0, 1) as usize;
                let bottom = param_or(params, 1, t.rows as u16) as usize;
                if top < bottom && bottom <= t.rows {
                    t.scroll_top = top - 1;
                    t.scroll_bottom = bottom - 1;
                    t.cur_x = 0;
                    t.cur_y = t.scroll_top;
                }
            }
            'm' => {
                let flat = flat_params(params);
                t.sgr(&flat);
            }
            'h' | 'l' => {
                let enable = action == 'h';
                for sub in params.iter() {
                    for &p in sub {
                        t.set_mode(private, p, enable);
                    }
                }
            }
            's' => t.saved_cursor = (t.cur_x, t.cur_y, t.pen),
            'u' => {
                let (x, y, pen) = t.saved_cursor;
                t.cur_x = x.min(t.cols - 1);
                t.cur_y = y.min(t.rows - 1);
                t.pen = pen;
            }
            'c' => {
                // DA — pretend to be a VT102
                t.responses.extend_from_slice(b"\x1b[?6c");
            }
            'n' => {
                let q = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(0);
                if q == 6 {
                    let resp = format!("\x1b[{};{}R", t.cur_y + 1, t.cur_x + 1);
                    t.responses.extend_from_slice(resp.as_bytes());
                } else if q == 5 {
                    t.responses.extend_from_slice(b"\x1b[0n");
                }
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        let t = &mut self.term;
        match byte {
            b'D' => t.linefeed(),
            b'E' => {
                t.cur_x = 0;
                t.linefeed();
            }
            b'M' => {
                // Reverse index
                if t.cur_y == t.scroll_top {
                    t.scroll_down(1);
                } else {
                    t.cur_y = t.cur_y.saturating_sub(1);
                }
            }
            b'7' => t.saved_cursor = (t.cur_x, t.cur_y, t.pen),
            b'8' => {
                let (x, y, pen) = t.saved_cursor;
                t.cur_x = x.min(t.cols - 1);
                t.cur_y = y.min(t.rows - 1);
                t.pen = pen;
            }
            b'c' => {
                // Full reset
                let (cols, rows) = (t.cols, t.rows);
                **t = Term::new(cols, rows);
            }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        // OSC 0/2 — window title
        if params.len() >= 2 && (params[0] == b"0" || params[0] == b"2") {
            self.term.title = String::from_utf8_lossy(params[1]).into_owned();
        }
    }

    fn hook(&mut self, _: &vte::Params, _: &[u8], _: bool, _: char) {}
    fn put(&mut self, _: u8) {}
    fn unhook(&mut self) {}
}
