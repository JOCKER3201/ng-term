//! Draw list — everything as triangles (one pipeline, one atlas).

use crate::font::{FontSystem, Glyph};
use crate::theme::Color;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

pub struct DrawList {
    pub verts: Vec<Vertex>,
}

impl DrawList {
    pub fn new() -> Self {
        DrawList { verts: Vec::with_capacity(1 << 16) }
    }

    pub fn clear(&mut self) {
        self.verts.clear();
    }

    fn push_quad(&mut self, p: [[f32; 2]; 4], uv: [[f32; 2]; 4], color: Color) {
        let c = color.to_array();
        let v = |i: usize| Vertex { pos: p[i], uv: uv[i], color: c };
        self.verts.extend_from_slice(&[v(0), v(1), v(2), v(0), v(2), v(3)]);
    }

    /// Arbitrary quadrilateral (vertices along the perimeter).
    pub fn quad(&mut self, p: [[f32; 2]; 4], color: Color) {
        let (u, v) = FontSystem::white_uv();
        self.push_quad(p, [[u, v]; 4], color);
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.quad([[x, y], [x + w, y], [x + w, y + h], [x, y + h]], color);
    }

    pub fn rect_outline(&mut self, x: f32, y: f32, w: f32, h: f32, t: f32, color: Color) {
        self.rect(x, y, w, t, color);
        self.rect(x, y + h - t, w, t, color);
        self.rect(x, y + t, t, h - 2.0 * t, color);
        self.rect(x + w - t, y + t, t, h - 2.0 * t, color);
    }

    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, t: f32, color: Color) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len = (dx * dx + dy * dy).sqrt().max(0.0001);
        let nx = -dy / len * t * 0.5;
        let ny = dx / len * t * 0.5;
        self.quad(
            [
                [x0 + nx, y0 + ny],
                [x1 + nx, y1 + ny],
                [x1 - nx, y1 - ny],
                [x0 - nx, y0 - ny],
            ],
            color,
        );
    }

    pub fn polyline(&mut self, pts: &[[f32; 2]], t: f32, color: Color, closed: bool) {
        if pts.len() < 2 {
            return;
        }
        for w in pts.windows(2) {
            self.line(w[0][0], w[0][1], w[1][0], w[1][1], t, color);
        }
        if closed {
            let a = pts[pts.len() - 1];
            let b = pts[0];
            self.line(a[0], a[1], b[0], b[1], t, color);
        }
    }

    /// Frame with clipped corners in the augmented-ui style (eDEX panels).
    pub fn chamfer_frame(&mut self, x: f32, y: f32, w: f32, h: f32, cut: f32, t: f32, color: Color) {
        let pts = [
            [x + cut, y],
            [x + w - cut, y],
            [x + w, y + cut],
            [x + w, y + h - cut],
            [x + w - cut, y + h],
            [x + cut, y + h],
            [x, y + h - cut],
            [x, y + cut],
        ];
        self.polyline(&pts, t, color, true);
    }

    fn glyph_quad(&mut self, g: &Glyph, pen_x: f32, baseline: f32, color: Color) {
        if g.w <= 0.0 {
            return;
        }
        let x0 = (pen_x + g.xmin).round();
        let y1 = (baseline - g.ymin).round(); // bitmap bottom
        let y0 = y1 - g.h;
        let x1 = x0 + g.w;
        self.push_quad(
            [[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
            [
                [g.u0, g.v0],
                [g.u1, g.v0],
                [g.u1, g.v1],
                [g.u0, g.v1],
            ],
            color,
        );
    }

    /// Draws text; (x, y) is the top-left corner of the text box. Returns width.
    pub fn text(
        &mut self,
        fs: &mut FontSystem,
        font: u8,
        px: f32,
        x: f32,
        y: f32,
        text: &str,
        color: Color,
        letter_spacing: f32,
    ) -> f32 {
        let (ascent, _) = fs.line_metrics(font, px);
        let baseline = y + ascent;
        let mut pen = x;
        for ch in text.chars() {
            if let Some(g) = fs.glyph(font, px, ch) {
                self.glyph_quad(&g, pen, baseline, color);
                pen += g.advance + letter_spacing;
            }
        }
        pen - x
    }

    /// Text horizontally centered on cx.
    #[allow(clippy::too_many_arguments)]
    pub fn text_center(
        &mut self,
        fs: &mut FontSystem,
        font: u8,
        px: f32,
        cx: f32,
        y: f32,
        text: &str,
        color: Color,
        letter_spacing: f32,
    ) {
        let w = fs.measure(font, px, text, letter_spacing);
        self.text(fs, font, px, cx - w / 2.0, y, text, color, letter_spacing);
    }

    /// Text right-aligned to the rx edge.
    #[allow(clippy::too_many_arguments)]
    pub fn text_right(
        &mut self,
        fs: &mut FontSystem,
        font: u8,
        px: f32,
        rx: f32,
        y: f32,
        text: &str,
        color: Color,
        letter_spacing: f32,
    ) {
        let w = fs.measure(font, px, text, letter_spacing);
        self.text(fs, font, px, rx - w, y, text, color, letter_spacing);
    }

    /// eDEX-style module header: underline with "whiskers" + text
    /// on the left and optionally on the right.
    #[allow(clippy::too_many_arguments)]
    pub fn module_title(
        &mut self,
        fs: &mut FontSystem,
        x: f32,
        y: f32,
        w: f32,
        px: f32,
        left: &str,
        right: &str,
        color: Color,
    ) {
        let line_c = color.alpha(0.3);
        let h = px * 1.75;
        self.text(fs, crate::font::FONT_UI, px, x + px * 0.6, y, left, color, px * 0.06);
        if !right.is_empty() {
            self.text_right(fs, crate::font::FONT_UI, px, x + w - px * 0.6, y, right, color, px * 0.06);
        }
        // bottom line
        self.line(x, y + h, x + w, y + h, 1.0, line_c);
        // whiskers at the ends
        self.line(x, y + h - px * 0.45, x, y + h, 1.0, line_c);
        self.line(x + w, y + h - px * 0.45, x + w, y + h, 1.0, line_c);
    }
}
