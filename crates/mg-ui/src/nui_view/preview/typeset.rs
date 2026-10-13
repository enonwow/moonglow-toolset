//! Text as the client draws it: whole-pixel advances and Nuklear's clamp.
use super::*;

/// How a text is drawn: its font (the skin's name for it), the canvas scale,
/// its color, where it sits in its rectangle and whether it wraps there.
#[derive(Clone, Copy)]
pub(in crate::nui_view) struct TextStyle<'a> {
    pub font: &'a str,
    pub scale: f32,
    pub color: Color32,
    pub align: Align2,
    pub wrap: bool,
}

pub(in crate::nui_view) fn text(
    p: &Painter,
    assets: &skin::Assets,
    rect: Rect,
    label: &str,
    style: TextStyle<'_>,
) {
    draw_text(p, assets, rect, label, style, true);
}

/// `clamp`: drawn by nk_draw_text, which cuts glyphs; an edit field only clips.
pub(super) fn draw_text(
    p: &Painter,
    assets: &skin::Assets,
    rect: Rect,
    label: &str,
    style: TextStyle<'_>,
    clamp: bool,
) {
    let TextStyle { font, scale, color, align, wrap } = style;
    if !rect.is_positive() || label.is_empty() {
        return;
    }
    let (font_id, spacing) = assets.font(p.ctx(), font, scale);
    let layout = |label: &str| {
        let mut job = native_job(p.ctx(), label, &font_id, spacing, color, None);
        job.wrap.max_width = if wrap { rect.width() } else { f32::INFINITY };
        p.layout_job(job)
    };
    let mut galley = layout(label);
    // nk_draw_text cuts a left-aligned line after the glyph that reaches its
    // end (nk_text_clamp; NWN EE 8193.37, np_misc: a 200-wide combo shows
    // "A very long dropdow"); centred and right-aligned text is clipped instead.
    if clamp
        && !wrap
        && align.x() == egui::Align::Min
        && !label.contains('\n')
        && galley.size().x > rect.width()
    {
        let mut width = 0.0;
        let fit = label
            .char_indices()
            .zip(native_advances(p.ctx(), label, &font_id, spacing))
            .find(|(_, advance)| {
                let past = width >= rect.width();
                width += advance;
                past
            })
            .map_or(label.len(), |((i, _), _)| i);
        galley = layout(&label[..fit]);
    }
    // NWN clips overflowing single-line text at the right edge; centering an
    // oversized galley would hide the start of a label instead.
    let mut position = align.align_size_within_rect(galley.size(), rect).min;
    position.x = position.x.max(rect.left());
    p.with_clip_rect(rect.intersect(p.clip_rect())).galley(position, galley, color);
}

/// One glyph's advance as the client draws it: the font's own plus the skin's
/// `spacing_h`, snapped to a whole pixel (`pixel_snap`). Where NWN EE 8193.37
/// wraps NuiText, and how long its lines are, follow this and not the
/// unsnapped advance.
pub(super) fn native_advances(
    ctx: &egui::Context,
    text: &str,
    font_id: &egui::FontId,
    spacing: f32,
) -> Vec<f32> {
    ctx.fonts_mut(|f| {
        text.chars()
            .map(|c| if c == '\n' { 0.0 } else { (f.glyph_width(font_id, c) + spacing).round() })
            .collect()
    })
}

/// Longer texts are laid out with the plain spacing: a section per glyph is
/// what the snapped advances cost.
pub(super) const SNAPPED_TEXT: usize = 8192;

/// A job whose glyphs advance by `native_advances`: egui's own advance, and
/// before each next glyph the space that makes up the difference.
pub(in crate::nui_view) fn native_job(
    ctx: &egui::Context,
    text: &str,
    font_id: &egui::FontId,
    spacing: f32,
    color: Color32,
    line_height: Option<f32>,
) -> egui::text::LayoutJob {
    let mut format =
        egui::TextFormat { font_id: font_id.clone(), color, line_height, ..Default::default() };
    if text.len() > SNAPPED_TEXT {
        format.extra_letter_spacing = spacing;
        return egui::text::LayoutJob::single_section(text.to_owned(), format);
    }
    let mut job = egui::text::LayoutJob::default();
    let mut lead = 0.0;
    ctx.fonts_mut(|f| {
        for c in text.chars() {
            job.append(c.encode_utf8(&mut [0; 4]), lead, format.clone());
            lead = if c == '\n' {
                0.0
            } else {
                let w = f.glyph_width(font_id, c);
                (w + spacing).round() - w
            };
        }
    });
    job
}

/// Text laid out as the client's NuiText lays it out: lines break at spaces
/// only, and a word wider than the whole line is cut where it stops fitting
/// (egui would also break after hyphens and underscores). Each line is `pitch`
/// high.
pub(super) fn native_text_layout(
    p: &Painter,
    text: &str,
    font_id: &egui::FontId,
    spacing: f32,
    pitch: f32,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut lines = String::with_capacity(text.len() + 16);
    for (n, paragraph) in text.split('\n').enumerate() {
        if n > 0 {
            lines.push('\n');
        }
        let advances = native_advances(p.ctx(), paragraph, font_id, spacing);
        lines.push_str(&native_wrap(paragraph, &advances, width));
    }
    p.layout_job(native_job(p.ctx(), &lines, font_id, spacing, color, Some(pitch)))
}

/// One paragraph with a line break wherever the client breaks it: Nuklear's
/// `nk_text_clamp` with a space as the only separator. A glyph is taken while
/// the line is still short of `width`, so a word stays on the line only if it
/// ends inside it, while a word cut for having no space before it may run one
/// glyph past the edge.
pub(super) fn native_wrap(paragraph: &str, advances: &[f32], width: f32) -> String {
    let chars: Vec<char> = paragraph.chars().collect();
    if width <= 0.0 {
        return paragraph.to_owned();
    }
    let mut out = String::with_capacity(paragraph.len() + 8);
    let mut start = 0;
    while start < chars.len() {
        let mut end = start;
        let mut used = 0.0;
        let mut after_space = None;
        while used < width && end < chars.len() {
            used += advances[end];
            if chars[end] == ' ' {
                after_space = Some(end + 1);
            }
            end += 1;
        }
        let cut = if end == chars.len() { end } else { after_space.unwrap_or(end) };
        if start > 0 {
            out.push('\n');
        }
        out.extend(&chars[start..cut]);
        start = cut;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nuklear's clamp, as NWN wraps NuiText: spaces only; a word stays only if
    /// it ends inside the line; a word with no space before it is cut a glyph
    /// past the edge; the text's last glyph may overflow too.
    #[test]
    pub(super) fn native_wrap_follows_nuklear_text_clamp() {
        let wrap =
            |text: &str, width: f32| native_wrap(text, &vec![10.0; text.chars().count()], width);
        assert_eq!(wrap("aaa bbbb", 65.0), "aaa \nbbbb");
        assert_eq!(wrap("aaa bbb", 65.0), "aaa bbb");
        assert_eq!(wrap("abcdefghij", 35.0), "abcd\nefgh\nij");
        assert_eq!(wrap("a-b-c-d e_f", 45.0), "a-b-c\n-d \ne_f");
        assert_eq!(wrap("Żółć żółć", 45.0), "Żółć \nżółć");
    }
}
