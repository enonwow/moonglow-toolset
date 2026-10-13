//! NuiChart: lines and columns as Nuklear draws them.
use super::*;

impl Renderer<'_> {
    pub(super) fn chart(&mut self, ui: &mut Ui, c: &Control<'_>) {
        let Control { s, a, scale, node, row, body, tint, font, .. } = *c;
        let p = &c.p;
        let response = &c.response;
        let mut hover = None;
        if let Some(slots) = val(&node["value"], s, row).as_array() {
            // Measured in the client (nui_chart_s, np_chart3): each series
            // scales between its own minimum and maximum; a line's points
            // are a width / count apart from the left edge; columns touch.
            let chart = body.shrink(4.0 * scale);
            if slots.is_empty() {
                c.label(body, "No chart data.", Align2::CENTER_CENTER, false);
            }
            for slot in slots {
                let Some(data) = val(&slot["data"], s, row).as_array() else {
                    continue;
                };
                let values: Vec<_> =
                    data.iter().filter_map(Value::as_f64).filter(|n| n.is_finite()).collect();
                let min = values.iter().copied().fold(f64::INFINITY, f64::min);
                let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                if !min.is_finite() || max <= min {
                    continue;
                }
                let color = rgba(val(&slot["color"], s, row), Color32::LIGHT_BLUE) * tint;
                let y = |v: f64| chart.bottom() - chart.height() * ((v - min) / (max - min)) as f32;
                let step = chart.width() / data.len().max(1) as f32;
                let points: Vec<_> = data
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| {
                        v.as_f64()
                            .filter(|n| n.is_finite())
                            .map(|v| pos2(chart.left() + i as f32 * step, y(v)))
                    })
                    .collect();
                if slot["type"] == 1 {
                    // Nuklear's nk_chart_push_column, as the client draws
                    // it (NWN EE 8193.37, np_chart3): a column is
                    // |v / range| high, stands at (v + |min|) / range from
                    // the bottom, or hangs (v - max) / range from the top
                    // when negative. With all values above zero they hang
                    // from the top; nothing keeps them inside the chart.
                    let (h, range) = (f64::from(chart.height()), max - min);
                    // Only the window or a group cuts them, not the chart.
                    let p = ui.painter();
                    for (i, v) in data.iter().enumerate() {
                        let Some(v) = v.as_f64().filter(|n| n.is_finite()) else {
                            continue;
                        };
                        let height = h * (v / range).abs();
                        let top = if v >= 0.0 {
                            f64::from(chart.bottom()) - h * (v + min.abs()) / range.abs()
                        } else {
                            f64::from(chart.top()) + h * ((v - max) / range).abs() - height
                        };
                        // Nuklear moves each column right by its index: a
                        // one-point gap between them.
                        let x = chart.left() + i as f32 * (step + scale);
                        let column =
                            Rect::from_min_size(pos2(x, top as f32), vec2(step, height as f32));
                        // The client lights the column under the pointer and
                        // shows its value (NWN EE 8193.37, np_chart2: "2.00").
                        let hovered = ui
                            .input(|i| i.pointer.hover_pos())
                            .is_some_and(|at| column.intersect(ui.clip_rect()).contains(at));
                        if hovered {
                            hover = Some(v);
                        }
                        p.rect_filled(column, 0, if hovered { Color32::WHITE } else { color });
                    }
                } else {
                    p.add(egui::Shape::line(points.clone(), Stroke::new(scale, color)));
                    for point in points {
                        p.rect_filled(
                            Rect::from_center_size(point, Vec2::splat(4.0 * scale)),
                            0,
                            color,
                        );
                    }
                }
            }
            // The native chart shows the first slot's legend only.
            if let Some(slot) = slots.first() {
                let legend = localized_string(&slot["legend"], s, row, a);
                text(
                    p,
                    a,
                    chart,
                    &legend,
                    TextStyle {
                        font,
                        scale,
                        color: rgba(val(&slot["color"], s, row), Color32::LIGHT_BLUE) * tint,
                        align: Align2::LEFT_TOP,
                        wrap: false,
                    },
                );
            }
        }
        if let Some(v) = hover {
            response.clone().on_hover_text_at_pointer(format!("{v:.2}"));
        }
    }
}
