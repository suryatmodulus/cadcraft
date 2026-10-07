//! MTEXT: inline-format parsing, word wrap and attachment.

use cadcraft_geom::{Bounds2, Vec2};

use crate::{layout_line, line_width};

/// Strip MTEXT formatting codes to plain text with `\n` paragraph breaks.
/// Stacked fractions `\S1/2;` become `1/2`; `\~` is a space; braces are dropped.
pub fn plain_mtext(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        match c {
            '\\' => {
                let code = chars.get(i + 1).copied().unwrap_or(' ');
                match code {
                    'P' => {
                        out.push('\n');
                        i += 2;
                    }
                    'N' => {
                        out.push('\n');
                        i += 2;
                    }
                    '~' => {
                        out.push(' ');
                        i += 2;
                    }
                    '\\' | '{' | '}' => {
                        out.push(code);
                        i += 2;
                    }
                    'S' => {
                        // \Snum^den; \Snum/den; \Snum#den;
                        let mut j = i + 2;
                        let mut stack = String::new();
                        while let Some(&ch) = chars.get(j) {
                            if ch == ';' {
                                break;
                            }
                            stack.push(match ch {
                                '^' | '#' => '/',
                                c => c,
                            });
                            j += 1;
                        }
                        out.push_str(stack.trim_end_matches('/'));
                        i = j + 1;
                    }
                    'L' | 'l' | 'O' | 'o' | 'K' | 'k' => i += 2,
                    // Codes with an argument terminated by ';' (\f, \F, \H, \W, \Q, \T, \A, \C, \c, \p).
                    _ => {
                        let mut j = i + 2;
                        while let Some(&ch) = chars.get(j) {
                            if ch == ';' {
                                break;
                            }
                            j += 1;
                        }
                        i = j + 1;
                    }
                }
            }
            '{' | '}' => i += 1,
            '\r' => i += 1,
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MTextLayout {
    pub strokes: Vec<Vec<Vec2>>,
    pub bounds: Bounds2,
    pub lines: Vec<String>,
}

/// Lay out MTEXT. `attach` 1..=9 (TL, TC, TR, ML, MC, MR, BL, BC, BR).
pub fn layout_mtext(contents: &str, insert: Vec2, height: f64, width: f64, attach: u8, rotation: f64, line_spacing: f64) -> MTextLayout {
    let h = if height > 0.0 && height.is_finite() { height } else { 1.0 };
    let text = plain_mtext(contents);
    let mut lines: Vec<String> = Vec::new();
    for para in text.split('\n') {
        if width > 0.0 {
            let mut cur = String::new();
            for word in para.split(' ') {
                let trial = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
                if !cur.is_empty() && line_width(&trial, h, 1.0) > width {
                    lines.push(std::mem::take(&mut cur));
                    cur = word.to_string();
                } else {
                    cur = trial;
                }
            }
            lines.push(cur);
        } else {
            lines.push(para.to_string());
        }
    }
    let pitch = h * 5.0 / 3.0 * if line_spacing > 0.0 && line_spacing.is_finite() { line_spacing } else { 1.0 };
    let block_w = if width > 0.0 { width } else { lines.iter().map(|l| line_width(l, h, 1.0)).fold(0.0, f64::max) };
    let n = lines.len().max(1) as f64;
    let block_h = h + pitch * (n - 1.0);
    let col = (attach.clamp(1, 9) - 1) % 3;
    let row = (attach.clamp(1, 9) - 1) / 3;
    // Top of the first line's cap height relative to the insert point.
    let top = match row {
        0 => 0.0,
        1 => block_h / 2.0,
        _ => block_h,
    };
    let mut out = MTextLayout { lines: lines.clone(), ..Default::default() };
    for (i, line) in lines.iter().enumerate() {
        let run = layout_line(line, h, 1.0, 0.0);
        let x = match col {
            0 => 0.0,
            1 => -run.width / 2.0,
            _ => -run.width,
        };
        let base_y = top - h - pitch * i as f64;
        for st in run.strokes {
            out.strokes.push(st.into_iter().map(|p| insert + (p + Vec2::new(x, base_y)).rotate(rotation)).collect());
        }
    }
    let x0 = match col {
        0 => 0.0,
        1 => -block_w / 2.0,
        _ => -block_w,
    };
    out.bounds = Bounds2::from_points(
        [Vec2::new(x0, top), Vec2::new(x0 + block_w, top), Vec2::new(x0 + block_w, top - block_h), Vec2::new(x0, top - block_h)]
            .map(|p| insert + p.rotate(rotation)),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_formatting() {
        assert_eq!(plain_mtext("{\\fArial|b1;Bold}\\Pline 2"), "Bold\nline 2");
        assert_eq!(plain_mtext("1\\S1/2;\""), "11/2\"");
        assert_eq!(plain_mtext("a\\~b\\\\c"), "a b\\c");
        assert_eq!(plain_mtext("\\H2.5x;Big"), "Big");
        assert_eq!(plain_mtext("unterminated \\H2"), "unterminated ");
    }

    #[test]
    fn wraps_to_width() {
        let l = layout_mtext("the quick brown fox jumps over the lazy dog", Vec2::ZERO, 1.0, 8.0, 1, 0.0, 1.0);
        assert!(l.lines.len() > 2);
        assert!(l.bounds.max.y.abs() < 1e-9);
        assert!((l.bounds.width() - 8.0).abs() < 1e-9);
    }

    #[test]
    fn middle_centre_attach() {
        let l = layout_mtext("AB", Vec2::new(5.0, 5.0), 1.0, 0.0, 5, 0.0, 1.0);
        let c = l.bounds.center();
        assert!((c.x - 5.0).abs() < 1e-9 && (c.y - 5.0).abs() < 1e-9);
    }
}
