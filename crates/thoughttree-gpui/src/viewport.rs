use thoughttree_gpui_model::Position;

pub const NODE_WIDTH: f64 = 170.;
pub const NODE_HEIGHT: f64 = 120.;

#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub pan: Position,
    pub zoom: f64,
    pub width: f64,
    pub height: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            pan: Position { x: 80., y: 60. },
            zoom: 1.,
            width: 1000.,
            height: 800.,
        }
    }
}

impl Viewport {
    pub fn screen(&self, position: Position) -> Position {
        Position {
            x: position.x * self.zoom + self.pan.x,
            y: position.y * self.zoom + self.pan.y,
        }
    }

    pub fn graph(&self, position: Position) -> Position {
        Position {
            x: (position.x - self.pan.x) / self.zoom,
            y: (position.y - self.pan.y) / self.zoom,
        }
    }

    pub fn zoom_at(&mut self, factor: f64, anchor: Position) {
        let graph = self.graph(anchor);
        self.zoom = (self.zoom * factor).clamp(0.1, 3.);
        self.pan = Position {
            x: anchor.x - graph.x * self.zoom,
            y: anchor.y - graph.y * self.zoom,
        };
    }

    pub fn center(&mut self, position: Position) {
        self.pan = Position {
            x: self.width / 2. - (position.x + NODE_WIDTH / 2.) * self.zoom,
            y: self.height / 2. - (position.y + NODE_HEIGHT / 2.) * self.zoom,
        };
    }

    pub fn fit(&mut self, positions: impl Iterator<Item = Position>) {
        let positions: Vec<_> = positions.collect();
        let Some(first) = positions.first() else {
            return;
        };
        let (mut left, mut top, mut right, mut bottom) = (first.x, first.y, first.x, first.y);
        for p in positions {
            left = left.min(p.x);
            top = top.min(p.y);
            right = right.max(p.x);
            bottom = bottom.max(p.y);
        }
        let width = right - left + NODE_WIDTH;
        let height = bottom - top + NODE_HEIGHT;
        self.zoom = ((self.width - 100.) / width)
            .min((self.height - 100.) / height)
            .clamp(0.1, 1.5);
        self.pan = Position {
            x: (self.width - width * self.zoom) / 2. - left * self.zoom,
            y: (self.height - height * self.zoom) / 2. - top * self.zoom,
        };
    }
}

/// Matches ReactFlow's alignment threshold in graph coordinates.
pub fn snap(
    position: Position,
    others: impl Iterator<Item = Position>,
) -> (Position, Vec<(bool, f64)>) {
    let mut result = position;
    let mut guides = Vec::new();
    for other in others {
        for (horizontal, value, target, extent) in [
            (false, position.x, other.x, NODE_WIDTH),
            (true, position.y, other.y, NODE_HEIGHT),
        ] {
            let candidate = [target, target + extent, target - extent]
                .into_iter()
                .find(|candidate| (value - candidate).abs() < 8.);
            if let Some(candidate) = candidate {
                if horizontal {
                    result.y = candidate;
                } else {
                    result.x = candidate;
                }
                guides.push((horizontal, candidate));
            }
        }
    }
    (result, guides)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_the_point_under_the_cursor_fixed_even_at_limits() {
        let mut view = Viewport::default();
        let anchor = Position { x: 331., y: 201. };
        let before = view.graph(anchor);
        view.zoom_at(100., anchor);
        assert_eq!(view.zoom, 3.);
        assert_eq!(view.graph(anchor), before);
        view.zoom_at(0.0001, anchor);
        assert!((view.graph(anchor).x - before.x).abs() < 0.001);
    }

    #[test]
    fn fitting_negative_positions_keeps_all_cards_inside_viewport() {
        let mut view = Viewport::default();
        let positions = [
            Position { x: -600., y: -200. },
            Position { x: 2000., y: 1800. },
        ];
        view.fit(positions.into_iter());
        for position in positions {
            let p = view.screen(position);
            assert!(p.x >= 49. && p.y >= 49.);
            assert!(p.x + NODE_WIDTH * view.zoom <= view.width - 49.);
            assert!(p.y + NODE_HEIGHT * view.zoom <= view.height - 49.);
        }
    }
}
