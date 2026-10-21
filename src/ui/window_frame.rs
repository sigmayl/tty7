//! Linux client-side window frame. Resize handling follows gpui-component's
//! `window_border` (Apache-2.0), with rounded corners on exposed edges.

use gpui::{
    AnyElement, App, Background, Corners, CursorStyle, Decorations, Edges, Hsla,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement, Pixels, Point, RenderOnce,
    ResizeEdge, Size, Styled as _, Tiling, Window, div, point, prelude::FluentBuilder as _, px,
};
use gpui_component::ActiveTheme as _;

use super::rounding::RoundedCorners as _;

const RADIUS: Pixels = px(12.);
const SHADOW: Pixels = px(12.);
const BORDER: Pixels = px(1.);
const RESIZE_HIT: Pixels = px(4.);

#[derive(IntoElement)]
pub(crate) struct WindowFrame {
    background: Background,
    children: Vec<AnyElement>,
}

impl WindowFrame {
    pub(crate) fn new(background: impl Into<Background>) -> Self {
        Self {
            background: background.into(),
            children: Vec::new(),
        }
    }
}

impl ParentElement for WindowFrame {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

fn corners(tiling: Tiling) -> Corners<Pixels> {
    Corners {
        top_left: if tiling.top || tiling.left {
            px(0.)
        } else {
            RADIUS
        },
        top_right: if tiling.top || tiling.right {
            px(0.)
        } else {
            RADIUS
        },
        bottom_left: if tiling.bottom || tiling.left {
            px(0.)
        } else {
            RADIUS
        },
        bottom_right: if tiling.bottom || tiling.right {
            px(0.)
        } else {
            RADIUS
        },
    }
}

fn frame_insets(tiling: Tiling) -> Edges<Pixels> {
    Edges {
        top: if tiling.top { px(0.) } else { SHADOW },
        bottom: if tiling.bottom { px(0.) } else { SHADOW },
        left: if tiling.left { px(0.) } else { SHADOW },
        right: if tiling.right { px(0.) } else { SHADOW },
    }
}

fn window_tiling(window: &Window, tiling: Tiling) -> Tiling {
    if window.is_fullscreen() || window.is_maximized() {
        Tiling::tiled()
    } else {
        tiling
    }
}

/// Position of the app's content, for tab-drop hit testing. Window persistence
/// still uses the platform's inner bounds (which exclude only the shadow).
pub(crate) fn content_insets(window: &Window) -> Edges<Pixels> {
    let Decorations::Client { tiling } = window.window_decorations() else {
        return Edges::all(px(0.));
    };
    let tiling = window_tiling(window, tiling);
    let frame = frame_insets(tiling);
    Edges {
        top: frame.top + if tiling.top { px(0.) } else { BORDER },
        bottom: frame.bottom + if tiling.bottom { px(0.) } else { BORDER },
        left: frame.left + if tiling.left { px(0.) } else { BORDER },
        right: frame.right + if tiling.right { px(0.) } else { BORDER },
    }
}

impl RenderOnce for WindowFrame {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Decorations::Client { tiling } = window.window_decorations() else {
            window.set_window_clip(None);
            // macOS, Windows, and Linux without a compositing window manager
            // keep the platform's native frame and corner treatment.
            return div()
                .size_full()
                .bg(self.background)
                .children(self.children)
                .into_any_element();
        };
        let tiling = window_tiling(window, tiling);
        // This must stay stable through maximize/restore, just as in
        // gpui-component: changing it makes the restored window grow.
        window.set_client_inset(SHADOW);
        let insets = frame_insets(tiling);
        let radii = corners(tiling);
        // Fullscreen/maximized window bounds retain the restore size; clipping
        // and resize hit testing need the current drawable surface instead.
        let size = window.viewport_size();
        window.set_window_clip(Some((
            gpui::Bounds {
                origin: point(insets.left, insets.top),
                size: gpui::size(
                    size.width - insets.left - insets.right,
                    size.height - insets.top - insets.bottom,
                ),
            },
            radii,
        )));

        div()
            .id("window-backdrop")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .pt(insets.top)
            .pb(insets.bottom)
            .pl(insets.left)
            .pr(insets.right)
            .on_mouse_down(MouseButton::Left, move |_, window, _| {
                if let Some(edge) = resize_edge(window.mouse_position(), size, tiling) {
                    window.start_window_resize(edge);
                }
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .rounded_corners(radii)
                    .bg(self.background)
                    .border_color(cx.theme().window_border)
                    .when(!tiling.top, |d| d.border_t(BORDER))
                    .when(!tiling.bottom, |d| d.border_b(BORDER))
                    .when(!tiling.left, |d| d.border_l(BORDER))
                    .when(!tiling.right, |d| d.border_r(BORDER))
                    .when(!tiling.is_tiled(), |d| {
                        d.shadow(vec![gpui::BoxShadow {
                            color: Hsla {
                                h: 0.,
                                s: 0.,
                                l: 0.,
                                a: 0.3,
                            },
                            blur_radius: SHADOW / 2.,
                            spread_radius: px(0.),
                            offset: point(px(0.), px(0.)),
                            inset: false,
                        }])
                    })
                    .child(div().size_full().overflow_hidden().children(self.children)),
            )
            // Cursor-only strips leave the backdrop in charge of starting the
            // resize, including when the pointer is just inside the frame.
            .child(
                div()
                    .absolute()
                    .size_full()
                    .children(resize_zones(size, tiling)),
            )
            .into_any_element()
    }
}

fn cursor(edge: ResizeEdge) -> CursorStyle {
    match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    }
}

fn resize_zones(size: Size<Pixels>, tiling: Tiling) -> Vec<AnyElement> {
    let insets = frame_insets(tiling);
    let left = insets.left;
    let right = size.width - insets.right;
    let top = insets.top;
    let bottom = size.height - insets.bottom;
    let band = RESIZE_HIT * 2.;
    let mut zones = Vec::new();
    let mut add = |edge, x, y, width, height| {
        zones.push(
            div()
                .absolute()
                .left(x - left)
                .top(y - top)
                .w(width)
                .h(height)
                .cursor(cursor(edge))
                .into_any_element(),
        );
    };
    if !tiling.top {
        add(
            ResizeEdge::Top,
            left - RESIZE_HIT,
            top - RESIZE_HIT,
            right - left + band,
            band,
        );
    }
    if !tiling.bottom {
        add(
            ResizeEdge::Bottom,
            left - RESIZE_HIT,
            bottom - RESIZE_HIT,
            right - left + band,
            band,
        );
    }
    if !tiling.left {
        add(
            ResizeEdge::Left,
            left - RESIZE_HIT,
            top - RESIZE_HIT,
            band,
            bottom - top + band,
        );
    }
    if !tiling.right {
        add(
            ResizeEdge::Right,
            right - RESIZE_HIT,
            top - RESIZE_HIT,
            band,
            bottom - top + band,
        );
    }
    // Last painted wins at an overlap, so corners follow the straight edges.
    if !tiling.top && !tiling.left {
        add(
            ResizeEdge::TopLeft,
            left - RESIZE_HIT,
            top - RESIZE_HIT,
            band,
            band,
        );
    }
    if !tiling.top && !tiling.right {
        add(
            ResizeEdge::TopRight,
            right - RESIZE_HIT,
            top - RESIZE_HIT,
            band,
            band,
        );
    }
    if !tiling.bottom && !tiling.left {
        add(
            ResizeEdge::BottomLeft,
            left - RESIZE_HIT,
            bottom - RESIZE_HIT,
            band,
            band,
        );
    }
    if !tiling.bottom && !tiling.right {
        add(
            ResizeEdge::BottomRight,
            right - RESIZE_HIT,
            bottom - RESIZE_HIT,
            band,
            band,
        );
    }
    zones
}

fn resize_edge(pos: Point<Pixels>, size: Size<Pixels>, tiling: Tiling) -> Option<ResizeEdge> {
    let insets = frame_insets(tiling);
    let left = insets.left;
    let right = size.width - insets.right;
    let top = insets.top;
    let bottom = size.height - insets.bottom;
    let within_x = pos.x >= left - RESIZE_HIT && pos.x <= right + RESIZE_HIT;
    let within_y = pos.y >= top - RESIZE_HIT && pos.y <= bottom + RESIZE_HIT;
    let on_left = !tiling.left && within_y && (pos.x - left).abs() <= RESIZE_HIT;
    let on_right = !tiling.right && within_y && (pos.x - right).abs() <= RESIZE_HIT;
    let on_top = !tiling.top && within_x && (pos.y - top).abs() <= RESIZE_HIT;
    let on_bottom = !tiling.bottom && within_x && (pos.y - bottom).abs() <= RESIZE_HIT;
    match (on_top, on_bottom, on_left, on_right) {
        (true, _, true, _) => Some(ResizeEdge::TopLeft),
        (true, _, _, true) => Some(ResizeEdge::TopRight),
        (_, true, true, _) => Some(ResizeEdge::BottomLeft),
        (_, true, _, true) => Some(ResizeEdge::BottomRight),
        (true, _, _, _) => Some(ResizeEdge::Top),
        (_, true, _, _) => Some(ResizeEdge::Bottom),
        (_, _, true, _) => Some(ResizeEdge::Left),
        (_, _, _, true) => Some(ResizeEdge::Right),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exposed_corners_are_rounded() {
        assert_eq!(corners(Tiling::default()), Corners::all(RADIUS));
        assert_eq!(corners(Tiling::tiled()), Corners::all(px(0.)));
        let left = corners(Tiling {
            left: true,
            ..Tiling::default()
        });
        assert_eq!(left.top_left, px(0.));
        assert_eq!(left.bottom_left, px(0.));
        assert_eq!(left.top_right, RADIUS);
        assert_eq!(left.bottom_right, RADIUS);
        let top = corners(Tiling {
            top: true,
            ..Tiling::default()
        });
        assert_eq!(top.top_left, px(0.));
        assert_eq!(top.top_right, px(0.));
        assert_eq!(top.bottom_left, RADIUS);
        assert_eq!(top.bottom_right, RADIUS);
    }

    #[test]
    fn resizing_uses_the_visible_frame_and_prefers_corners() {
        let size = gpui::size(px(800.), px(600.));
        let hit = |x, y, tiling| resize_edge(point(px(x), px(y)), size, tiling);
        let floating = Tiling::default();
        assert_eq!(hit(12., 12., floating), Some(ResizeEdge::TopLeft));
        assert_eq!(hit(788., 12., floating), Some(ResizeEdge::TopRight));
        assert_eq!(hit(12., 588., floating), Some(ResizeEdge::BottomLeft));
        assert_eq!(hit(788., 588., floating), Some(ResizeEdge::BottomRight));
        assert_eq!(hit(400., 12., floating), Some(ResizeEdge::Top));
        assert_eq!(hit(12., 300., floating), Some(ResizeEdge::Left));
        assert_eq!(hit(400., 300., floating), None);
        assert_eq!(hit(1., 1., floating), None);
        assert_eq!(hit(12., 12., Tiling::tiled()), None);
        let left = Tiling {
            left: true,
            ..floating
        };
        assert_eq!(hit(0., 300., left), None);
        assert_eq!(hit(0., 12., left), Some(ResizeEdge::Top));
        assert!(resize_zones(size, Tiling::tiled()).is_empty());
    }
}
