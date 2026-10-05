use base64::Engine;
use std::time::Duration;
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

use gpui::{prelude::*, *};
use thoughttree_core::vault::files::{
    extension_mime, is_raster_image, FilePreview, FilePreviewResponse,
};
use thoughttree_desktop::VaultFileStatus;
use thoughttree_gpui_model::{GraphEdge, GraphNode, ImageAttachment, Position, Role};

use crate::{
    theme,
    viewport::{snap, Viewport, NODE_HEIGHT, NODE_WIDTH},
};

#[derive(Clone)]
pub enum GraphEvent {
    Select(Vec<String>),
    SelectEdge(String),
    EdgeContext(String),
    Preview(String, bool),
    TogglePreview(String),
    Move(BTreeMap<String, Position>),
    Connect(String, String),
    New(Position, Option<String>),
    Generate(String),
    Reply(String),
    Context(Option<String>, Position),
    DropFiles(Vec<std::path::PathBuf>, Position),
    DropImages(Vec<std::path::PathBuf>, String),
}

enum Gesture {
    Pan {
        start: Position,
        origin: Position,
    },
    Nodes {
        start: Position,
        origins: BTreeMap<String, Position>,
    },
    Select {
        start: Position,
        current: Position,
    },
    Connect {
        source: String,
        cursor: Position,
    },
    Minimap {
        map: Viewport,
        origin: Position,
    },
}

pub struct GraphCanvas {
    pub viewport: Viewport,
    nodes: Vec<(GraphNode, Position)>,
    edges: Vec<GraphEdge>,
    selection: BTreeSet<String>,
    streaming: BTreeSet<String>,
    blocked: BTreeSet<String>,
    generation_blocked: BTreeSet<String>,
    gesture: Option<Gesture>,
    guides: Vec<(bool, f64)>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    minimap_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub interactive: bool,
    selected_edge: Option<String>,
    flash: Option<String>,
    flash_task: Option<Task<()>>,
    file_previews: BTreeMap<String, FilePreviewResponse>,
    file_status: BTreeMap<String, Result<VaultFileStatus, String>>,
    file_errors: BTreeMap<String, String>,
    file_images: BTreeMap<String, std::sync::Arc<Image>>,
    inline_images: BTreeMap<(String, usize), CachedAttachment>,
}

struct CachedAttachment {
    source: ImageAttachment,
    image: std::sync::Arc<Image>,
}

impl EventEmitter<GraphEvent> for GraphCanvas {}

impl GraphCanvas {
    #[cfg(test)]
    pub(crate) fn guide_count(&self) -> usize {
        self.guides.len()
    }

    pub fn new() -> Self {
        Self {
            viewport: Viewport::default(),
            nodes: Vec::new(),
            edges: Vec::new(),
            selection: BTreeSet::new(),
            streaming: BTreeSet::new(),
            blocked: BTreeSet::new(),
            generation_blocked: BTreeSet::new(),
            gesture: None,
            guides: Vec::new(),
            bounds: Rc::new(Cell::new(Bounds::default())),
            minimap_bounds: Rc::new(Cell::new(Bounds::default())),
            interactive: true,
            selected_edge: None,
            flash: None,
            flash_task: None,
            file_previews: BTreeMap::new(),
            file_status: BTreeMap::new(),
            file_errors: BTreeMap::new(),
            file_images: BTreeMap::new(),
            inline_images: BTreeMap::new(),
        }
    }

    pub fn set_file_previews(
        &mut self,
        previews: BTreeMap<String, FilePreviewResponse>,
        status: BTreeMap<String, Result<VaultFileStatus, String>>,
        errors: BTreeMap<String, String>,
        cx: &mut Context<Self>,
    ) {
        for image in self.file_images.values() {
            ImageSource::Image(image.clone()).remove_asset(cx);
        }
        self.file_images = previews
            .iter()
            .filter_map(|(id, response)| {
                file_preview_image(response).map(|image| (id.clone(), image))
            })
            .collect();
        self.file_previews = previews;
        self.file_status = status;
        self.file_errors = errors;
        cx.notify();
    }

    pub fn set_graph(
        &mut self,
        nodes: Vec<(GraphNode, Position)>,
        edges: Vec<GraphEdge>,
        selected: BTreeSet<String>,
        streaming: BTreeSet<String>,
        blocked: BTreeSet<String>,
        cx: &mut Context<Self>,
    ) {
        self.cache_inline_images(&nodes, cx);
        self.retain_file_previews(&nodes, cx);
        self.nodes = nodes;
        self.edges = edges;
        self.selection = selected;
        self.streaming = streaming;
        self.blocked = blocked;
        cx.notify();
    }

    fn retain_file_previews(&mut self, nodes: &[(GraphNode, Position)], cx: &mut Context<Self>) {
        let ids: BTreeSet<_> = nodes
            .iter()
            .filter(|(node, _)| node.role() == Role::File)
            .map(|(node, _)| &node.id)
            .collect();
        self.file_previews.retain(|id, _| ids.contains(id));
        self.file_status.retain(|id, _| ids.contains(id));
        self.file_errors.retain(|id, _| ids.contains(id));
        self.file_images.retain(|id, image| {
            if ids.contains(id) {
                true
            } else {
                ImageSource::Image(image.clone()).remove_asset(cx);
                false
            }
        });
    }

    fn cache_inline_images(&mut self, nodes: &[(GraphNode, Position)], cx: &mut Context<Self>) {
        let mut previous = std::mem::take(&mut self.inline_images);
        for (node, _) in nodes {
            for (index, attachment) in node.images().iter().take(3).enumerate() {
                let key = (node.id.clone(), index);
                let old = previous.remove(&key);
                let next = match old {
                    Some(cached) if cached.source == *attachment => Some(cached),
                    old => {
                        remove_cached_attachment(old, cx);
                        cached_attachment(attachment)
                    }
                };
                if let Some(next) = next {
                    self.inline_images.insert(key, next);
                }
            }
        }
        for (_, old) in previous {
            ImageSource::Image(old.image).remove_asset(cx);
        }
    }

    pub fn set_generation_blocked(&mut self, blocked: BTreeSet<String>, cx: &mut Context<Self>) {
        self.generation_blocked = blocked;
        cx.notify();
    }

    pub fn fit(&mut self, cx: &mut Context<Self>) {
        self.measure();
        self.viewport.fit(self.nodes.iter().map(|(_, p)| *p));
        cx.notify();
    }

    pub fn jump(&mut self, id: &str, cx: &mut Context<Self>) {
        self.measure();
        self.viewport.zoom = 1.;
        if let Some((_, p)) = self.nodes.iter().find(|(node, _)| node.id == id) {
            self.viewport.center(*p);
        }
        self.flash = Some(id.to_string());
        self.flash_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(850))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.flash = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn measure(&mut self) {
        let bounds = self.bounds.get();
        if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
            return;
        }
        self.viewport.width = f32::from(bounds.size.width) as f64;
        self.viewport.height = f32::from(bounds.size.height) as f64;
    }

    fn local(&self, point: Point<Pixels>) -> Position {
        let origin = self.bounds.get().origin;
        Position {
            x: f32::from(point.x - origin.x) as f64,
            y: f32::from(point.y - origin.y) as f64,
        }
    }

    fn drop_files(
        &mut self,
        paths: Vec<std::path::PathBuf>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.measure();
        cx.emit(GraphEvent::DropFiles(
            paths,
            self.viewport.graph(self.local(window.mouse_position())),
        ));
    }

    fn node_down(&mut self, id: String, ev: &MouseDownEvent, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if !self.interactive {
            return;
        }
        self.selected_edge = None;
        if let Some(Gesture::Connect { source, .. }) = self.gesture.take() {
            cx.emit(GraphEvent::Connect(source, id));
            return;
        }
        if ev.modifiers.shift || ev.modifiers.platform {
            if !self.selection.remove(&id) {
                self.selection.insert(id.clone());
            }
        } else if !self.selection.contains(&id) {
            self.selection = BTreeSet::from([id.clone()]);
        }
        cx.emit(GraphEvent::Select(self.selection.iter().cloned().collect()));
        if ev.click_count == 2 {
            let edit = self
                .nodes
                .iter()
                .any(|(n, _)| n.id == id && n.role() == Role::User);
            cx.emit(GraphEvent::Preview(id, edit));
        } else {
            let origins = self
                .nodes
                .iter()
                .filter(|(n, _)| self.selection.contains(&n.id) && !self.blocked.contains(&n.id))
                .map(|(n, p)| (n.id.clone(), *p))
                .collect();
            self.gesture = Some(Gesture::Nodes {
                start: self.local(ev.position),
                origins,
            });
        }
        cx.notify();
    }

    fn edge_at(&self, point: Position) -> Option<String> {
        self.edges.iter().find_map(|edge| {
            let source = self.nodes.iter().find(|(n, _)| n.id == edge.source)?.1;
            let target = self.nodes.iter().find(|(n, _)| n.id == edge.target)?.1;
            let s = self.viewport.screen(Position {
                x: source.x + NODE_WIDTH / 2.,
                y: source.y + NODE_HEIGHT,
            });
            let t = self.viewport.screen(Position {
                x: target.x + NODE_WIDTH / 2.,
                y: target.y,
            });
            let mid = (s.y + t.y) / 2.;
            (0..=80)
                .any(|index| {
                    let u = index as f64 / 80.;
                    let v = 1. - u;
                    let x = v * v * v * s.x
                        + 3. * v * v * u * s.x
                        + 3. * v * u * u * t.x
                        + u * u * u * t.x;
                    let y = v * v * v * s.y
                        + 3. * v * v * u * mid
                        + 3. * v * u * u * mid
                        + u * u * u * t.y;
                    (point.x - x).hypot(point.y - y) < 8.
                })
                .then(|| edge.id.clone())
        })
    }

    fn move_pointer(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) {
        let local = self.local(ev.position);
        match &mut self.gesture {
            Some(Gesture::Pan { start, origin }) => {
                self.viewport.pan = Position {
                    x: origin.x + local.x - start.x,
                    y: origin.y + local.y - start.y,
                }
            }
            Some(Gesture::Nodes { start, origins }) if ev.dragging() => {
                let delta = Position {
                    x: (local.x - start.x) / self.viewport.zoom,
                    y: (local.y - start.y) / self.viewport.zoom,
                };
                let other_positions: Vec<_> = self
                    .nodes
                    .iter()
                    .filter(|(n, _)| !origins.contains_key(&n.id))
                    .map(|(_, p)| *p)
                    .collect();
                self.guides.clear();
                for (node, position) in &mut self.nodes {
                    if let Some(origin) = origins.get(&node.id) {
                        let raw = Position {
                            x: origin.x + delta.x,
                            y: origin.y + delta.y,
                        };
                        let (snapped, guides) = snap(raw, other_positions.iter().copied());
                        *position = snapped;
                        self.guides.extend(guides);
                    }
                }
            }
            Some(Gesture::Select { current, .. }) => *current = local,
            Some(Gesture::Connect { cursor, .. }) => *cursor = local,
            Some(Gesture::Minimap { map, origin }) => {
                let world = map.graph(Position {
                    x: local.x - origin.x,
                    y: local.y - origin.y,
                });
                self.viewport.center(Position {
                    x: world.x - NODE_WIDTH / 2.,
                    y: world.y - NODE_HEIGHT / 2.,
                });
            }
            _ => return,
        }
        cx.notify();
    }

    fn release(&mut self, ev: &MouseUpEvent, cx: &mut Context<Self>) {
        match self.gesture.take() {
            Some(Gesture::Nodes { origins, .. }) => {
                let changed: BTreeMap<_, _> = self
                    .nodes
                    .iter()
                    .filter(|(n, p)| origins.get(&n.id).is_some_and(|old| old != p))
                    .map(|(n, p)| (n.id.clone(), *p))
                    .collect();
                if !changed.is_empty() {
                    cx.emit(GraphEvent::Move(changed));
                }
            }
            Some(Gesture::Select { start, current }) => {
                self.selection = self
                    .nodes
                    .iter()
                    .filter(|(_, p)| {
                        let p = self.viewport.screen(*p);
                        p.x + NODE_WIDTH * self.viewport.zoom >= start.x.min(current.x)
                            && p.x <= start.x.max(current.x)
                            && p.y + NODE_HEIGHT * self.viewport.zoom >= start.y.min(current.y)
                            && p.y <= start.y.max(current.y)
                    })
                    .map(|(n, _)| n.id.clone())
                    .collect();
                cx.emit(GraphEvent::Select(self.selection.iter().cloned().collect()));
            }
            Some(Gesture::Connect { source, .. }) => {
                let local = self.viewport.graph(self.local(ev.position));
                let target = self
                    .nodes
                    .iter()
                    .find(|(_, p)| {
                        local.x >= p.x
                            && local.x <= p.x + NODE_WIDTH
                            && local.y >= p.y
                            && local.y <= p.y + NODE_HEIGHT
                    })
                    .map(|(n, _)| n.id.clone());
                if let Some(target) = target {
                    cx.emit(GraphEvent::Connect(source, target));
                } else {
                    cx.emit(GraphEvent::New(local, Some(source)));
                }
            }
            _ => {}
        }
        self.guides.clear();
        cx.notify();
    }

    fn node_card(
        &self,
        node: &GraphNode,
        position: Position,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = node.id.clone();
        let selected = self.selection.contains(&id);
        let streaming = self.streaming.contains(&id);
        let blocked = self.blocked.contains(&id);
        let user = node.role() == Role::User;
        let action_blocked = blocked || (user && self.generation_blocked.contains(&id));
        let zoom = self.viewport.zoom as f32;
        let p = self.viewport.screen(position);
        let label = match node.role() {
            Role::User => "User".to_string(),
            Role::Assistant => node
                .assistant_data()
                .and_then(|a| a.provider)
                .map(|p| p.as_str())
                .unwrap_or("Assistant")
                .to_string(),
            Role::File => node
                .file_data()
                .and_then(|f| f.name.rsplit('.').next())
                .unwrap_or("FILE")
                .to_uppercase(),
        };
        let title = collapsed_text(node);
        let title = if title.is_empty() {
            if streaming {
                "Waiting for response…"
            } else {
                "Double-click to edit in panel"
            }
            .into()
        } else {
            title
        };
        let down_id = id.clone();
        let context_id = id.clone();
        let source_id = id.clone();
        let action_id = id.clone();
        let preview_id = id.clone();
        let drop_id = id.clone();
        let foreground = if user { theme::bg() } else { theme::text() };
        div()
            .id(SharedString::from(id.clone()))
            .absolute()
            .left(px(p.x as f32))
            .top(px(p.y as f32))
            .w(px(NODE_WIDTH as f32 * zoom))
            .h(px(NODE_HEIGHT as f32 * zoom))
            .rounded(px(6. * zoom))
            .bg(if user {
                theme::accent()
            } else {
                theme::surface()
            })
            .text_color(foreground)
            .border_2()
            .border_color(if self.flash.as_ref() == Some(&id) || selected {
                gpui::white()
            } else if node.role() == Role::File {
                theme::edge()
            } else {
                theme::accent()
            })
            .when(blocked && !streaming, |d| d.opacity(0.65))
            .shadow_sm()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev, _, cx| this.node_down(down_id.clone(), ev, cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    cx.emit(GraphEvent::Context(
                        Some(context_id.clone()),
                        this.viewport.graph(this.local(ev.position)),
                    ));
                }),
            )
            .when(user, |element| {
                element.on_drop(cx.listener(move |_, paths: &ExternalPaths, _, cx| {
                    let images: Vec<_> = paths
                        .paths()
                        .iter()
                        .filter(|path| is_raster_image(extension_mime(path)))
                        .cloned()
                        .collect();
                    if images.is_empty() {
                        return;
                    }
                    cx.stop_propagation();
                    if !blocked {
                        cx.emit(GraphEvent::DropImages(images, drop_id.clone()));
                    }
                }))
            })
            .child(
                div()
                    .absolute()
                    .top(px(-5.))
                    .left(px((NODE_WIDTH as f32 / 2. - 4.) * zoom))
                    .size(px(8.))
                    .rounded_full()
                    .bg(theme::accent())
                    .when(node.role() == Role::File, |d| d.invisible()),
            )
            .child(
                div()
                    .p(px(9. * zoom))
                    .size_full()
                    .flex()
                    .flex_col()
                    .gap(px(5. * zoom))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(10. * zoom))
                            .child(label)
                            .child(
                                div()
                                    .id(SharedString::from(format!("preview-{id}")))
                                    .debug_selector(|| format!("preview-{id}"))
                                    .child("▾")
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |_, _, _, cx| {
                                            cx.stop_propagation();
                                            cx.emit(GraphEvent::TogglePreview(preview_id.clone()));
                                        }),
                                    ),
                            ),
                    )
                    .when(!node.images().is_empty(), |d| {
                        d.child(
                            div().flex().gap_1().h(px(28. * zoom)).children(
                                (0..node.images().len().min(3))
                                    .filter_map(|index| {
                                        self.inline_images.get(&(id.clone(), index))
                                    })
                                    .map(|cached| {
                                        img(cached.image.clone())
                                            .size(px(28. * zoom))
                                            .object_fit(ObjectFit::Contain)
                                    }),
                            ),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .text_size(px(12. * zoom))
                            .when(node.role() == Role::File, |d| {
                                d.debug_selector(|| format!("file-name-{}: {title}", node.id))
                            })
                            .child(title)
                            .when(node.role() == Role::File, |d| {
                                d.child(self.file_detail(node, zoom))
                            }),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("action-{id}")))
                            .debug_selector(|| format!("action-{id}"))
                            .text_size(px(10. * zoom))
                            .border_t_1()
                            .border_color(if user { theme::edge() } else { theme::border() })
                            .pt(px(3. * zoom))
                            .child(if streaming {
                                "Generating…"
                            } else if user {
                                "Generate  ⌘↵"
                            } else if node.role() == Role::File {
                                "File context"
                            } else {
                                "Continue  ↵"
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |_, _, _, cx| {
                                    cx.stop_propagation();
                                    if !action_blocked {
                                        cx.emit(if user {
                                            GraphEvent::Generate(action_id.clone())
                                        } else {
                                            GraphEvent::Reply(action_id.clone())
                                        });
                                    }
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(format!("source-{id}")))
                    .absolute()
                    .bottom(px(-5.))
                    .left(px((NODE_WIDTH as f32 / 2. - 4.) * zoom))
                    .size(px(10.))
                    .rounded_full()
                    .bg(theme::accent())
                    .cursor_crosshair()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if this.interactive && !blocked {
                                this.gesture = Some(Gesture::Connect {
                                    source: source_id.clone(),
                                    cursor: this.local(ev.position),
                                });
                                cx.notify();
                            }
                        }),
                    ),
            )
            .into_any_element()
    }
}

impl Render for GraphCanvas {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.measure();
        let view = self.viewport;
        let bounds_cell = self.bounds.clone();
        let owner = cx.weak_entity();
        let positions: BTreeMap<_, _> =
            self.nodes.iter().map(|(n, p)| (n.id.clone(), *p)).collect();
        let edges: Vec<_> = self
            .edges
            .iter()
            .filter_map(|e| {
                Some((
                    *positions.get(&e.source)?,
                    *positions.get(&e.target)?,
                    self.selected_edge.as_ref() == Some(&e.id),
                ))
            })
            .collect();
        let guides = self.guides.clone();
        let selection = match self.gesture {
            Some(Gesture::Select { start, current }) => Some((start, current)),
            _ => None,
        };
        let connection = match &self.gesture {
            Some(Gesture::Connect { source, cursor }) => {
                positions.get(source).map(|p| (*p, *cursor))
            }
            _ => None,
        };
        let layer = canvas(
            move |bounds, _, cx| {
                let resized = bounds_cell.get().size != bounds.size;
                bounds_cell.set(bounds);
                if resized {
                    let _ = owner.update(cx, |this, cx| {
                        this.measure();
                        cx.notify();
                    });
                }
            },
            move |bounds, _, window, _| {
                let to_point = |p: Position| {
                    point(
                        bounds.origin.x + px(p.x as f32),
                        bounds.origin.y + px(p.y as f32),
                    )
                };
                for (source, target, selected) in &edges {
                    let s = view.screen(Position {
                        x: source.x + NODE_WIDTH / 2.,
                        y: source.y + NODE_HEIGHT,
                    });
                    let t = view.screen(Position {
                        x: target.x + NODE_WIDTH / 2.,
                        y: target.y,
                    });
                    let middle = (s.y + t.y) / 2.;
                    let mut path = PathBuilder::stroke(px(1.8));
                    path.move_to(to_point(s));
                    path.cubic_bezier_to(
                        to_point(t),
                        to_point(Position { x: s.x, y: middle }),
                        to_point(Position { x: t.x, y: middle }),
                    );
                    let color = if *selected {
                        theme::accent()
                    } else {
                        theme::edge()
                    };
                    if let Ok(path) = path.build() {
                        window.paint_path(path, color);
                    }
                }
                if let Some((source, target)) = connection {
                    let start = view.screen(Position {
                        x: source.x + NODE_WIDTH / 2.,
                        y: source.y + NODE_HEIGHT,
                    });
                    let mut path = PathBuilder::stroke(px(2.));
                    path.move_to(to_point(start));
                    path.line_to(to_point(target));
                    if let Ok(path) = path.build() {
                        window.paint_path(path, theme::accent());
                    }
                }
                for (horizontal, p) in &guides {
                    let mut path = PathBuilder::stroke(px(1.));
                    if *horizontal {
                        let y = view.screen(Position { x: 0., y: *p }).y;
                        path.move_to(to_point(Position { x: 0., y }));
                        path.line_to(to_point(Position {
                            x: f32::from(bounds.size.width) as f64,
                            y,
                        }));
                    } else {
                        let x = view.screen(Position { x: *p, y: 0. }).x;
                        path.move_to(to_point(Position { x, y: 0. }));
                        path.line_to(to_point(Position {
                            x,
                            y: f32::from(bounds.size.height) as f64,
                        }));
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, theme::accent());
                    }
                }
                if let Some((start, end)) = selection {
                    let rect = Bounds::new(
                        to_point(Position {
                            x: start.x.min(end.x),
                            y: start.y.min(end.y),
                        }),
                        size(
                            px((start.x - end.x).abs() as f32),
                            px((start.y - end.y).abs() as f32),
                        ),
                    );
                    window.paint_quad(fill(rect, rgba(0xc0facc22)));
                }
            },
        )
        .size_full();
        let cards: Vec<_> = self
            .nodes
            .clone()
            .iter()
            .filter(|(_, p)| {
                let p = view.screen(*p);
                p.x + NODE_WIDTH * view.zoom >= 0.
                    && p.y + NODE_HEIGHT * view.zoom >= 0.
                    && p.x <= view.width
                    && p.y <= view.height
            })
            .map(|(n, p)| self.node_card(n, *p, cx))
            .collect();
        div()
            .id("graph-canvas")
            .debug_selector(|| "graph-canvas".into())
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme::bg())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                    let local = this.local(ev.position);
                    if let Some(id) = this.interactive.then(|| this.edge_at(local)).flatten() {
                        this.selected_edge = Some(id.clone());
                        this.selection.clear();
                        cx.emit(GraphEvent::SelectEdge(id));
                        cx.notify();
                        return;
                    }
                    this.selected_edge = None;
                    if ev.click_count == 2 {
                        cx.emit(GraphEvent::New(this.viewport.graph(local), None));
                    } else if this.interactive && ev.modifiers.shift {
                        this.gesture = Some(Gesture::Select {
                            start: local,
                            current: local,
                        });
                    } else {
                        this.gesture = Some(Gesture::Pan {
                            start: local,
                            origin: this.viewport.pan,
                        });
                        this.selection.clear();
                        cx.emit(GraphEvent::Select(Vec::new()));
                    }
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                    let local = this.local(ev.position);
                    if let Some(id) = this.edge_at(local) {
                        cx.emit(GraphEvent::EdgeContext(id));
                    } else {
                        cx.emit(GraphEvent::Context(None, this.viewport.graph(local)));
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, ev, _, cx| this.move_pointer(ev, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, ev, _, cx| this.release(ev, cx)),
            )
            .on_scroll_wheel(cx.listener(|this, ev: &ScrollWheelEvent, _, cx| {
                this.measure();
                let delta = ev.delta.pixel_delta(px(20.));
                if ev.modifiers.control || ev.modifiers.platform {
                    this.viewport.zoom_at(
                        (-f32::from(delta.y) as f64 * 0.005).exp(),
                        this.local(ev.position),
                    );
                } else {
                    this.viewport.pan.x += f32::from(delta.x) as f64;
                    this.viewport.pan.y += f32::from(delta.y) as f64;
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.drop_files(paths.paths().to_vec(), window, cx)
            }))
            .child(layer)
            .children(cards)
            .child(
                div()
                    .absolute()
                    .left_4()
                    .bottom_4()
                    .flex()
                    .gap_1()
                    .bg(theme::surface())
                    .rounded_md()
                    .p_1()
                    .child(control("zoom-in", "+", cx, |s, cx| {
                        s.measure();
                        s.viewport.zoom_at(
                            1.2,
                            Position {
                                x: s.viewport.width / 2.,
                                y: s.viewport.height / 2.,
                            },
                        );
                        cx.notify();
                    }))
                    .child(control("zoom-out", "−", cx, |s, cx| {
                        s.measure();
                        s.viewport.zoom_at(
                            1. / 1.2,
                            Position {
                                x: s.viewport.width / 2.,
                                y: s.viewport.height / 2.,
                            },
                        );
                        cx.notify();
                    }))
                    .child(control("fit", "Fit", cx, |s, cx| s.fit(cx)))
                    .child(control(
                        "lock",
                        if self.interactive { "Lock" } else { "Unlock" },
                        cx,
                        |s, cx| {
                            s.interactive = !s.interactive;
                            s.gesture = None;
                            s.guides.clear();
                            cx.notify();
                        },
                    ))
                    .child(
                        div()
                            .text_xs()
                            .p_2()
                            .text_color(theme::muted())
                            .child(format!("{:.0}%", view.zoom * 100.)),
                    ),
            )
            .child(self.minimap(cx))
    }
}

fn file_preview_image(response: &FilePreviewResponse) -> Option<std::sync::Arc<Image>> {
    let FilePreview::Image {
        data, mime_type, ..
    } = &response.preview
    else {
        return None;
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    let format = ImageFormat::from_mime_type(mime_type)?;
    Some(std::sync::Arc::new(Image::from_bytes(format, bytes)))
}

fn cached_attachment(source: &ImageAttachment) -> Option<CachedAttachment> {
    let format = ImageFormat::from_mime_type(&source.mime_type)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&source.data)
        .ok()?;
    Some(CachedAttachment {
        source: source.clone(),
        image: std::sync::Arc::new(Image::from_bytes(format, bytes)),
    })
}

fn remove_cached_attachment(cached: Option<CachedAttachment>, cx: &mut App) {
    if let Some(cached) = cached {
        ImageSource::Image(cached.image).remove_asset(cx);
    }
}

fn collapsed_text(node: &GraphNode) -> String {
    if let Some(file) = node.file_data() {
        return file.name.clone();
    }
    if node.content.encode_utf16().count() <= 100 {
        return node.content.clone();
    }
    if let Some(summary) = node.summary.as_ref().filter(|summary| !summary.is_empty()) {
        return summary.clone();
    }
    let mut remaining = 30;
    let prefix: String = node
        .content
        .chars()
        .take_while(|ch| {
            let keep = ch.len_utf16() <= remaining;
            remaining = remaining.saturating_sub(ch.len_utf16());
            keep
        })
        .collect();
    format!("{prefix}...")
}

fn control(
    id: &'static str,
    label: &str,
    cx: &Context<GraphCanvas>,
    action: impl Fn(&mut GraphCanvas, &mut Context<GraphCanvas>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .debug_selector(|| id.into())
        .px_2()
        .py_1()
        .text_sm()
        .cursor_pointer()
        .hover(|d| d.bg(theme::raised()))
        .child(label.to_owned())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                action(this, cx);
            }),
        )
}

impl GraphCanvas {
    fn file_detail(&self, node: &GraphNode, zoom: f32) -> AnyElement {
        let Some(file) = node.file_data() else {
            return div().into_any_element();
        };
        let state = match self.file_status.get(&node.id) {
            None => "Checking file…".to_owned(),
            Some(Ok(VaultFileStatus::Missing)) => "Missing file".to_owned(),
            Some(Ok(VaultFileStatus::Invalid)) => "Outside notes directory".to_owned(),
            Some(Err(_)) => "File unavailable".to_owned(),
            Some(Ok(VaultFileStatus::Ok { stat, .. }))
                if stat.size != file.seen_size || stat.modified_epoch_ms != file.seen_mtime =>
            {
                "Changed on disk · Reload file".to_owned()
            }
            Some(Ok(_)) => format!(
                "{} bytes · {}",
                file.size,
                if is_raster_image(&file.mime_type) {
                    "Image attachment"
                } else {
                    "File reference"
                }
            ),
        };
        let state = self
            .file_errors
            .get(&node.id)
            .map(|error| {
                if error.starts_with("too_large:") {
                    "Image too large".to_owned()
                } else {
                    "Preview unavailable".to_owned()
                }
            })
            .unwrap_or(state);
        let mut detail = div()
            .debug_selector(|| format!("file-state-{}: {state}", node.id))
            .text_size(px(9. * zoom))
            .text_color(theme::muted())
            .child(state);
        if let Some(preview) = self.file_previews.get(&node.id) {
            match &preview.preview {
                FilePreview::Text { excerpt, .. } => {
                    detail = detail.child(
                        div()
                            .debug_selector(|| format!("file-text-{}", node.id))
                            .h(px(25. * zoom))
                            .overflow_hidden()
                            .child(excerpt.clone()),
                    )
                }
                FilePreview::Image { .. } => {
                    detail = detail.when_some(self.file_images.get(&node.id), |d, image| {
                        d.child(
                            div()
                                .debug_selector(|| format!("file-image-{}", node.id))
                                .child(
                                    img(image.clone())
                                        .h(px(28. * zoom))
                                        .max_w_full()
                                        .object_fit(ObjectFit::Contain),
                                ),
                        )
                    })
                }
                FilePreview::None => {}
            }
        }
        detail.into_any_element()
    }

    fn minimap(&self, cx: &Context<Self>) -> impl IntoElement {
        let mut mini = Viewport {
            width: 180.,
            height: 120.,
            ..Default::default()
        };
        mini.fit(self.nodes.iter().map(|(_, p)| *p));
        let viewport_origin = mini.screen(self.viewport.graph(Position::default()));
        let measured_bounds = self.minimap_bounds.clone();
        let measure = canvas(
            move |bounds, _, _| measured_bounds.set(bounds),
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let cards = self.nodes.iter().map(|(node, p)| {
            let p = mini.screen(*p);
            div()
                .absolute()
                .left(px(p.x as f32))
                .top(px(p.y as f32))
                .w(px((NODE_WIDTH * mini.zoom) as f32))
                .h(px((NODE_HEIGHT * mini.zoom) as f32))
                .bg(if node.role() == Role::User {
                    theme::accent()
                } else {
                    theme::edge()
                })
        });
        div()
            .id("minimap")
            .absolute()
            .bottom_4()
            .right_4()
            .w(px(180.))
            .h(px(120.))
            .rounded_md()
            .border_1()
            .border_color(theme::border())
            .bg(theme::surface())
            .overflow_hidden()
            .child(
                div()
                    .debug_selector(|| "minimap".into())
                    .relative()
                    .size_full()
                    .child(measure)
                    .children(cards)
                    .child(
                        div()
                            .absolute()
                            .left(px(viewport_origin.x as f32))
                            .top(px(viewport_origin.y as f32))
                            .w(px(
                                (self.viewport.width / self.viewport.zoom * mini.zoom) as f32
                            ))
                            .h(px(
                                (self.viewport.height / self.viewport.zoom * mini.zoom) as f32
                            ))
                            .border_1()
                            .border_color(theme::accent())
                            .bg(rgba(0xc0facc18)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.measure();
                            let origin = this.local(this.minimap_bounds.get().origin);
                            let pointer = this.local(ev.position);
                            let point = mini.graph(Position {
                                x: pointer.x - origin.x,
                                y: pointer.y - origin.y,
                            });
                            this.viewport.center(Position {
                                x: point.x - NODE_WIDTH / 2.,
                                y: point.y - NODE_HEIGHT / 2.,
                            });
                            this.gesture = Some(Gesture::Minimap { map: mini, origin });
                            cx.notify();
                        }),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use crate::interaction_tests::workspace;
    use base64::Engine;
    use gpui::TestAppContext;
    use thoughttree_gpui_model::{ImageAttachment, NodeKind};

    #[gpui::test]
    fn real_file_cards_render_metadata_previews_and_double_click_opens_the_file_panel(
        cx: &mut TestAppContext,
    ) {
        use gpui::{point, px, size, Modifiers, MouseButton, MouseDownEvent};
        use thoughttree_core::vault::files::FilePreview;
        use thoughttree_desktop::VaultFileStatus;
        use thoughttree_gpui_model::{FileData, GraphNode, Position};

        let (directory, workspace, cx, _events) = workspace(cx);
        cx.simulate_resize(size(px(1440.), px(940.)));
        std::fs::write(directory.path().join("vault/note.md"), "Text preview.").unwrap();
        image::RgbImage::from_pixel(2, 3, image::Rgb([10, 30, 50]))
            .save(directory.path().join("vault/image.png"))
            .unwrap();
        workspace.update(cx, |this, cx| {
            for (index, (id, path)) in [("text", "note.md"), ("image", "image.png")]
                .into_iter()
                .enumerate()
            {
                let VaultFileStatus::Ok {
                    stat,
                    mime_type,
                    name,
                } = this.desktop.stat_vault_file(path).unwrap()
                else {
                    panic!("fixture exists")
                };
                this.editor.project.graph.add_node(
                    GraphNode::file(
                        id,
                        FileData {
                            path: path.into(),
                            name,
                            mime_type,
                            size: stat.size,
                            seen_size: stat.size,
                            seen_mtime: stat.modified_epoch_ms,
                        },
                        1.,
                    ),
                    Position {
                        x: index as f64 * 240.,
                        y: 420.,
                    },
                );
            }
            this.refresh(cx);
            this.refresh_files(cx);
        });
        cx.run_until_parked();
        for selector in [
            "file-name-text: note.md",
            "file-name-image: image.png",
            "file-state-text: 13 bytes · File reference",
            "file-text-text",
            "file-image-image",
        ] {
            let bounds = cx.debug_bounds(selector).unwrap();
            assert!(bounds.size.width > px(0.) && bounds.size.height > px(0.));
        }
        workspace.read_with(cx, |this, cx| {
            let canvas = this.canvas.read(cx);
            assert!(matches!(&canvas.file_previews["text"].preview, FilePreview::Text { excerpt, truncated: false } if excerpt == "Text preview."));
            assert!(matches!(&canvas.file_previews["image"].preview, FilePreview::Image { width: 2, height: 3, mime_type, .. } if mime_type == "image/png"));
            let file = this.editor.project.graph.nodes["image"].file_data().unwrap();
            assert_eq!(file.mime_type, "image/png");
            assert_eq!(file.size, std::fs::metadata(directory.path().join("vault/image.png")).unwrap().len());
            assert!(canvas.file_images.contains_key("image"));
        });
        for (id, preview_selector) in [
            ("text", "file-preview-text"),
            ("image", "file-preview-image"),
        ] {
            let bounds = cx.debug_bounds("graph-canvas").unwrap();
            let screen = workspace.read_with(cx, |this, cx| {
                let p = this.editor.project.graph.layout[id];
                this.canvas.read(cx).viewport.screen(Position {
                    x: p.x + 40.,
                    y: p.y + 40.,
                })
            });
            let position = bounds.origin + point(px(screen.x as f32), px(screen.y as f32));
            cx.simulate_event(MouseDownEvent {
                position,
                button: MouseButton::Left,
                click_count: 2,
                ..Default::default()
            });
            cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::default());
            cx.run_until_parked();
            workspace.read_with(cx, |this, _| {
                assert_eq!(this.preview_id.as_deref(), Some(id));
                assert!(!this.editing);
                assert!(this.file_preview.as_ref().unwrap().is_ok());
            });
            for selector in ["file-name", "file-metadata", preview_selector] {
                let bounds = cx.debug_bounds(selector).unwrap();
                assert!(bounds.size.width > px(0.) && bounds.size.height > px(0.));
            }
            cx.simulate_keystrokes("escape");
        }
    }

    #[gpui::test]
    fn file_cards_render_distinct_loading_error_changed_and_ready_labels(cx: &mut TestAppContext) {
        use gpui::{px, size};
        use std::collections::BTreeMap;
        use thoughttree_core::vault::files::FileStat;
        use thoughttree_desktop::VaultFileStatus;
        use thoughttree_gpui_model::{FileData, GraphNode, Position};

        let (_directory, workspace, cx, _events) = workspace(cx);
        cx.simulate_resize(size(px(1440.), px(940.)));
        let ready = |size, modified_epoch_ms| {
            Ok(VaultFileStatus::Ok {
                stat: FileStat {
                    size,
                    modified_epoch_ms,
                },
                mime_type: "text/markdown".into(),
                name: "note.md".into(),
            })
        };
        workspace.update(cx, |this, cx| {
            for (index, id) in [
                "loading",
                "missing",
                "invalid",
                "changed",
                "unavailable",
                "large",
                "ready",
                "preview-error",
            ]
            .into_iter()
            .enumerate()
            {
                this.editor.project.graph.add_node(
                    GraphNode::file(
                        id,
                        FileData {
                            path: format!("{id}.md"),
                            name: format!("{id}.md"),
                            mime_type: "text/markdown".into(),
                            size: 4,
                            seen_size: 4,
                            seen_mtime: 1,
                        },
                        1.,
                    ),
                    Position {
                        x: (index % 4) as f64 * 220.,
                        y: 420. + (index / 4) as f64 * 180.,
                    },
                );
            }
            this.refresh(cx);
            this.canvas.update(cx, |canvas, cx| {
                canvas.set_file_previews(
                    BTreeMap::new(),
                    BTreeMap::from([
                        ("missing".into(), Ok(VaultFileStatus::Missing)),
                        ("invalid".into(), Ok(VaultFileStatus::Invalid)),
                        ("changed".into(), ready(5, 2)),
                        ("unavailable".into(), Err("Vault unavailable".into())),
                        ("large".into(), ready(4, 1)),
                        ("ready".into(), ready(4, 1)),
                        ("preview-error".into(), ready(4, 1)),
                    ]),
                    BTreeMap::from([
                        ("large".into(), "too_large: image dimensions".into()),
                        ("preview-error".into(), "Image decoding failed".into()),
                    ]),
                    cx,
                );
                canvas.fit(cx);
            });
        });
        cx.run_until_parked();
        for label in [
            "file-state-loading: Checking file…",
            "file-state-missing: Missing file",
            "file-state-invalid: Outside notes directory",
            "file-state-changed: Changed on disk · Reload file",
            "file-state-unavailable: File unavailable",
            "file-state-large: Image too large",
            "file-state-ready: 4 bytes · File reference",
            "file-state-preview-error: Preview unavailable",
        ] {
            let bounds = cx
                .debug_bounds(label)
                .unwrap_or_else(|| panic!("missing rendered label {label}"));
            assert!(bounds.size.width > px(0.) && bounds.size.height > px(0.));
        }
    }

    #[gpui::test]
    fn native_file_drop_callback_transforms_pointer_coordinates_before_fanning_out(
        cx: &mut TestAppContext,
    ) {
        use gpui::{point, px, Modifiers};
        use thoughttree_gpui_model::Position;

        let (directory, workspace, cx, _events) = workspace(cx);
        let mut paths = Vec::new();
        for name in ["first.md", "second.md"] {
            let path = directory.path().join("vault").join(name);
            std::fs::write(&path, name).unwrap();
            paths.push(path);
        }
        workspace.update(cx, |this, cx| {
            this.canvas.update(cx, |canvas, cx| {
                canvas.viewport.pan = Position { x: 37., y: -29. };
                canvas.viewport.zoom = 1.5;
                cx.notify();
            })
        });
        cx.run_until_parked();
        let bounds = cx.debug_bounds("graph-canvas").unwrap();
        // World (80, 120) becomes local (157, 151) at this pan and zoom.
        cx.simulate_mouse_move(
            bounds.origin + point(px(157.), px(151.)),
            None,
            Modifiers::default(),
        );
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.canvas.update(cx, |canvas, cx| {
                    // This is the same callback the ExternalPaths drop adapter calls.
                    canvas.drop_files(paths, window, cx);
                })
            })
        });
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| {
            let files: Vec<_> = this
                .editor
                .project
                .graph
                .nodes
                .values()
                .filter(|node| node.file_data().is_some())
                .collect();
            assert_eq!(files.len(), 2);
            for (index, (node, name)) in files.iter().zip(["first.md", "second.md"]).enumerate() {
                assert_eq!(node.file_data().unwrap().path, name);
                assert_eq!(
                    this.editor.project.graph.layout[&node.id],
                    Position {
                        x: 80. + index as f64 * 24.,
                        y: 120. + index as f64 * 24.,
                    }
                );
            }
            assert!(this.editor.is_dirty());
        });
    }

    #[gpui::test]
    fn deleting_a_file_evicts_its_canvas_preview_status_error_and_image(cx: &mut TestAppContext) {
        use std::collections::BTreeMap;
        use thoughttree_core::vault::files::{FileInfo, FilePreview, FilePreviewResponse};
        use thoughttree_gpui_model::{FileData, GraphNode, Position};

        let (_directory, workspace, cx, _events) = workspace(cx);
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        workspace.update(cx, |this, cx| {
            this.editor.project.graph.add_node(
                GraphNode::file(
                    "file",
                    FileData {
                        path: "image.png".into(),
                        name: "image.png".into(),
                        mime_type: "image/png".into(),
                        size: 64,
                        seen_size: 64,
                        seen_mtime: 0,
                    },
                    1.,
                ),
                Position::default(),
            );
            this.editor.selection.insert("file".into());
            this.refresh(cx);
            this.canvas.update(cx, |canvas, cx| {
                canvas.set_file_previews(
                    BTreeMap::from([(
                        "file".into(),
                        FilePreviewResponse {
                            info: FileInfo {
                                name: "image.png".into(),
                                mime_type: "image/png".into(),
                                size: 64,
                                modified_epoch_ms: 0,
                            },
                            preview: FilePreview::Image {
                                data: base64::engine::general_purpose::STANDARD
                                    .encode(bytes.into_inner()),
                                mime_type: "image/png".into(),
                                width: 2,
                                height: 2,
                            },
                        },
                    )]),
                    BTreeMap::from([("file".into(), Err("status".into()))]),
                    BTreeMap::from([("file".into(), "preview".into())]),
                    cx,
                )
            });
        });
        assert!(workspace.read_with(cx, |this, cx| this
            .canvas
            .read(cx)
            .file_images
            .contains_key("file")));
        cx.simulate_keystrokes("backspace");
        workspace.read_with(cx, |this, cx| {
            assert!(!this.editor.project.graph.nodes.contains_key("file"));
            let canvas = this.canvas.read(cx);
            assert!(canvas.file_previews.is_empty());
            assert!(canvas.file_status.is_empty());
            assert!(canvas.file_errors.is_empty());
            assert!(canvas.file_images.is_empty());
        });
    }

    #[test]
    fn short_cards_show_exact_content_and_long_cards_use_summary() {
        let mut node = thoughttree_gpui_model::GraphNode::user("a", "Short body", 1.);
        node.summary = Some("Old summary".into());
        assert_eq!(super::collapsed_text(&node), "Short body");
        node.content = "Long body ".repeat(20);
        assert_eq!(super::collapsed_text(&node), "Old summary");
        node.summary = None;
        assert_eq!(
            super::collapsed_text(&node),
            format!("{}...", "Long body ".repeat(3))
        );
    }

    #[gpui::test]
    fn inline_attachment_images_reuse_assets_until_the_attachment_changes(cx: &mut TestAppContext) {
        let (_directory, workspace, cx, _events) = workspace(cx);
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        workspace.update(cx, |this, cx| {
            let NodeKind::User(user) = &mut this.editor.project.graph.nodes["question"].kind else {
                unreachable!()
            };
            user.images.push(ImageAttachment {
                data: base64::engine::general_purpose::STANDARD.encode(bytes.into_inner()),
                mime_type: "image/png".into(),
                name: Some("fixture.png".into()),
            });
            this.refresh(cx);
        });
        let image = workspace.read_with(cx, |this, cx| {
            this.canvas.read(cx).inline_images[&("question".into(), 0)]
                .image
                .clone()
        });
        workspace.update(cx, |this, cx| {
            this.editor.project.graph.nodes["answer"]
                .content
                .push_str(" streamed");
            this.refresh(cx);
        });
        let reused = workspace.read_with(cx, |this, cx| {
            this.canvas.read(cx).inline_images[&("question".into(), 0)]
                .image
                .clone()
        });
        assert!(std::sync::Arc::ptr_eq(&image, &reused));
        workspace.update(cx, |this, cx| {
            let NodeKind::User(user) = &mut this.editor.project.graph.nodes["question"].kind else {
                unreachable!()
            };
            user.images.clear();
            this.refresh(cx);
        });
        assert!(workspace.read_with(cx, |this, cx| this.canvas.read(cx).inline_images.is_empty()));
    }
}
