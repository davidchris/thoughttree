//! DAG canvas — the GPUI equivalent of `src/components/Graph/index.tsx`.
//!
//! Each `GraphNode` renders as an absolutely-positioned card. Edges are drawn
//! as cubic Bézier curves on a single `canvas()` element painted underneath
//! the cards. Node drag/click are handled per-card; the canvas itself owns
//! pan offset.

use crate::graph::{
    types::{GraphNode, NodeRole, Position},
    NodeId,
};
use crate::state::AppState;
use crate::theme;
use gpui::{
    canvas, div, point, prelude::*, px, Context, Div, DragMoveEvent, Entity, MouseButton,
    ParentElement, PathBuilder, Pixels, Point, Render, Styled, Window,
};

pub struct CanvasView {
    state: Entity<AppState>,
    pan: Point<Pixels>,
    dragging_node: Option<(NodeId, Point<Pixels>)>,
}

impl CanvasView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            pan: point(px(0.0), px(0.0)),
            dragging_node: None,
        }
    }

    fn render_node(&self, node: &GraphNode, position: Position, cx: &Context<Self>) -> impl IntoElement {
        let app = self.state.read(cx);
        let selected = app.selected.as_ref() == Some(&node.id);
        let streaming = app.streaming.contains(&node.id);

        let bg = if selected {
            theme::BG_NODE_SELECTED
        } else {
            match node.role {
                NodeRole::User => theme::BG_NODE_USER,
                NodeRole::Assistant => theme::BG_NODE_ASSISTANT,
            }
        };

        let role_label = match node.role {
            NodeRole::User => "User",
            NodeRole::Assistant => node
                .provider
                .as_deref()
                .unwrap_or("Assistant"),
        };

        let preview = preview_text(&node.content, 220);
        let node_id = node.id.clone();
        let node_id_for_click = node.id.clone();

        div()
            .id(node.id.clone())
            .absolute()
            .left(px(position.x) + self.pan.x)
            .top(px(position.y) + self.pan.y)
            .w(px(theme::NODE_WIDTH))
            .min_h(px(theme::NODE_MIN_HEIGHT))
            .p(px(10.0))
            .bg(bg)
            .border_1()
            .border_color(if selected { theme::ACCENT } else { theme::BORDER })
            .rounded(px(8.0))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.state.update(cx, |s, cx| {
                        s.selected = Some(node_id_for_click.clone());
                        cx.notify();
                    });
                }),
            )
            .on_drag(node_id.clone(), |id, _, _, cx| cx.new(|_| DragHandle(id.clone())))
            .on_drag_move(cx.listener(
                move |this, ev: &DragMoveEvent<DragHandle>, _window, cx| {
                    let delta = ev.bounds.origin;
                    this.state.update(cx, |s, cx| {
                        s.set_position(
                            &node_id,
                            Position {
                                x: f32::from(delta.x),
                                y: f32::from(delta.y),
                            },
                        );
                        cx.notify();
                    });
                },
            ))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::TEXT_DIM)
                            .child(role_label.to_string()),
                    )
                    .child(if streaming {
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::ACCENT)
                            .child("● streaming")
                    } else {
                        div()
                    }),
            )
            .child(
                div()
                    .pt(px(6.0))
                    .text_size(px(12.0))
                    .text_color(theme::TEXT)
                    .child(if preview.is_empty() {
                        "(empty)".into()
                    } else {
                        preview
                    }),
            )
    }
}

/// Marker entity passed through GPUI's drag system to identify which node is
/// being moved. GPUI's drag API needs an `Entity<T>` payload.
struct DragHandle(NodeId);

fn preview_text(s: &str, max: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max {
        trimmed.to_string()
    } else {
        let cut: String = trimmed.chars().take(max).collect();
        format!("{cut}…")
    }
}

impl Render for CanvasView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.state.read(cx);

        // Snapshot edges with endpoint positions so the painted closure doesn't
        // re-borrow the entity across the painter.
        let edges: Vec<(Position, Position)> = app
            .graph
            .edges
            .iter()
            .filter_map(|e| {
                let s = app.graph.layout.get(&e.source)?;
                let t = app.graph.layout.get(&e.target)?;
                Some((*s, *t))
            })
            .collect();

        let pan = self.pan;

        let edge_layer = canvas(
            |_bounds, _window, _cx| (),
            move |_bounds, _prepaint, window, _cx| {
                for (s, t) in &edges {
                    // Anchor at bottom-center of source, top-center of target.
                    let sx = px(s.x + theme::NODE_WIDTH / 2.0) + pan.x;
                    let sy = px(s.y + theme::NODE_MIN_HEIGHT) + pan.y;
                    let tx = px(t.x + theme::NODE_WIDTH / 2.0) + pan.x;
                    let ty = px(t.y) + pan.y;
                    let mid_y = (sy + ty) / 2.0;

                    let mut path = PathBuilder::stroke(px(1.5));
                    path.move_to(point(sx, sy));
                    path.cubic_bezier_to(
                        point(tx, ty),
                        point(sx, mid_y),
                        point(tx, mid_y),
                    );
                    if let Ok(p) = path.build() {
                        window.paint_path(p, theme::TEXT_DIM);
                    }
                }
            },
        )
        .size_full();

        // Snapshot the (id, node, layout) tuples so we can iterate without
        // holding a borrow during render.
        let cards: Vec<(GraphNode, Position)> = app
            .graph
            .nodes
            .iter()
            .filter_map(|(id, node)| {
                let pos = app.graph.layout.get(id).copied()?;
                Some((node.clone(), pos))
            })
            .collect();

        let mut container = div()
            .relative()
            .size_full()
            .bg(theme::BG_APP)
            .overflow_hidden()
            .child(edge_layer);

        for (node, pos) in cards {
            container = container.child(self.render_node(&node, pos, cx));
        }

        container
    }
}
