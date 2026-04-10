//! A generic [`egui`] [`Widget`](egui::Widget) for laying out and exploring any directed acyclic
//! graph (DAG).
//!
//! # Overview
//!
//! [`DagView`] accepts a set of nodes and edges and renders an interactive graph in an egui panel.
//! Nodes are laid out in topological order (sources at the top), with edges drawn as curved lines
//! between them.  The user can pan and zoom the view.
//!
//! # Example
//!
//! ```no_run
//! use egui_dag_view::{DagView, NodeId};
//!
//! let mut nodes: Vec<NodeId> = (0u64..5).map(NodeId).collect();
//! // edges: 0→1, 0→2, 1→3, 2→3, 3→4
//! let edges: Vec<(NodeId, NodeId)> = vec![
//!     (NodeId(0), NodeId(1)),
//!     (NodeId(0), NodeId(2)),
//!     (NodeId(1), NodeId(3)),
//!     (NodeId(2), NodeId(3)),
//!     (NodeId(3), NodeId(4)),
//! ];
//! let mut view = DagView::new(nodes, edges);
//! // In your egui update loop:
//! // ui.add(&mut view);
//! ```

use egui::{Painter, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, Widget};
use std::collections::HashMap;

/// An opaque identifier for a node in the DAG.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub u64);

/// Configuration options for [`DagView`].
#[derive(Clone, Debug)]
pub struct DagViewConfig {
    /// Pixel width of each node box.
    pub node_width: f32,
    /// Pixel height of each node box.
    pub node_height: f32,
    /// Horizontal spacing between columns.
    pub column_spacing: f32,
    /// Vertical spacing between rows.
    pub row_spacing: f32,
}

impl Default for DagViewConfig {
    fn default() -> Self {
        Self {
            node_width: 120.0,
            node_height: 30.0,
            column_spacing: 40.0,
            row_spacing: 20.0,
        }
    }
}

/// Internal computed layout position for a node.
#[derive(Clone, Debug)]
#[allow(dead_code)] // `rank` and `column` are part of the layout model for future use
struct NodeLayout {
    id: NodeId,
    /// Rank (row) in the topological layout — 0 is the topmost (source) level.
    rank: usize,
    /// Position within the rank (column index).
    column: usize,
    /// Pixel-space centre of the node box (relative to the view origin).
    centre: Pos2,
}

/// A generic [`Widget`] that renders an interactive DAG view.
///
/// Nodes are positioned using a simple topological rank assignment: a node's rank is one more than
/// the maximum rank of all its predecessors.  Within each rank nodes are arranged left-to-right in
/// the order they were first encountered during the topological sort.
pub struct DagView {
    nodes: Vec<NodeId>,
    edges: Vec<(NodeId, NodeId)>,
    /// Current pan offset (in pixels).
    pan: Vec2,
    /// Current zoom factor.
    zoom: f32,
    config: DagViewConfig,
    /// Cached layout — recomputed lazily when `None`.
    layout: Option<Vec<NodeLayout>>,
    /// Optional per-node label callback.  When `None`, the node's id is rendered as a decimal.
    labels: HashMap<NodeId, String>,
}

impl DagView {
    /// Create a new `DagView` with the given nodes and edges.
    ///
    /// `edges` is a list of `(from, to)` pairs where `from` is a predecessor of `to`.
    pub fn new(nodes: Vec<NodeId>, edges: Vec<(NodeId, NodeId)>) -> Self {
        Self {
            nodes,
            edges,
            pan: Vec2::ZERO,
            zoom: 1.0,
            config: DagViewConfig::default(),
            layout: None,
            labels: HashMap::new(),
        }
    }

    /// Attach a human-readable label to a node.  The label is shown inside the node box.
    pub fn with_label(mut self, id: NodeId, label: impl Into<String>) -> Self {
        self.labels.insert(id, label.into());
        self
    }

    /// Override the default layout configuration.
    pub fn with_config(mut self, config: DagViewConfig) -> Self {
        self.config = config;
        self.layout = None; // invalidate cached layout
        self
    }

    // ── Layout computation ──────────────────────────────────────────────────

    /// Compute (or return the cached) layout for all nodes.
    fn compute_layout(&mut self) -> &[NodeLayout] {
        if self.layout.is_none() {
            self.layout = Some(self.build_layout());
        }
        self.layout.as_deref().unwrap()
    }

    fn build_layout(&self) -> Vec<NodeLayout> {
        // Build adjacency maps.
        let mut predecessors: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        let mut successors: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for &id in &self.nodes {
            predecessors.entry(id).or_default();
            successors.entry(id).or_default();
        }
        for &(from, to) in &self.edges {
            successors.entry(from).or_default().push(to);
            predecessors.entry(to).or_default().push(from);
        }

        // Assign ranks: rank[n] = 1 + max rank of all predecessors (0 for sources).
        let mut rank: HashMap<NodeId, usize> = HashMap::new();
        // Process in topological order (Kahn's algorithm).
        let mut in_degree: HashMap<NodeId, usize> = self.nodes.iter().map(|&n| (n, 0)).collect();
        for &(_, to) in &self.edges {
            *in_degree.entry(to).or_insert(0) += 1;
        }
        let mut queue: std::collections::VecDeque<NodeId> = self
            .nodes
            .iter()
            .filter(|&&n| in_degree[&n] == 0)
            .copied()
            .collect();
        for &n in &self.nodes {
            if in_degree[&n] == 0 {
                rank.insert(n, 0);
            }
        }
        while let Some(node) = queue.pop_front() {
            let r = rank[&node];
            for &next in successors.get(&node).unwrap_or(&vec![]) {
                let entry = rank.entry(next).or_insert(0);
                *entry = (*entry).max(r + 1);
                let deg = in_degree.entry(next).or_insert(0);
                *deg = deg.saturating_sub(1);
                if *deg == 0 {
                    queue.push_back(next);
                }
            }
        }

        // Group nodes by rank and assign column indices.
        let mut by_rank: HashMap<usize, Vec<NodeId>> = HashMap::new();
        for &n in &self.nodes {
            let r = rank.get(&n).copied().unwrap_or(0);
            by_rank.entry(r).or_default().push(n);
        }
        // Sort columns deterministically by node id.
        for v in by_rank.values_mut() {
            v.sort_by_key(|n| n.0);
        }

        let cfg = &self.config;
        let cell_w = cfg.node_width + cfg.column_spacing;
        let cell_h = cfg.node_height + cfg.row_spacing;

        let mut layouts = Vec::with_capacity(self.nodes.len());
        let mut column_map: HashMap<NodeId, usize> = HashMap::new();
        for (&r, nodes_in_rank) in &by_rank {
            for (col, &n) in nodes_in_rank.iter().enumerate() {
                column_map.insert(n, col);
                let cx = col as f32 * cell_w + cfg.node_width / 2.0;
                let cy = r as f32 * cell_h + cfg.node_height / 2.0;
                layouts.push(NodeLayout {
                    id: n,
                    rank: r,
                    column: col,
                    centre: Pos2::new(cx, cy),
                });
            }
        }
        layouts
    }

    // ── Rendering ───────────────────────────────────────────────────────────

    fn draw(&self, painter: &Painter, origin: Pos2, layout: &[NodeLayout]) {
        let cfg = &self.config;
        let zoom = self.zoom;

        // Build a lookup: NodeId → centre.
        let centres: HashMap<NodeId, Pos2> = layout
            .iter()
            .map(|nl| (nl.id, nl.centre))
            .collect();

        // Draw edges.
        let edge_stroke = Stroke::new(1.5 * zoom, painter.ctx().global_style().visuals.text_color());
        for &(from, to) in &self.edges {
            if let (Some(&fc), Some(&tc)) = (centres.get(&from), centres.get(&to)) {
                let p0 = origin + fc.to_vec2() * zoom;
                let p1 = origin + tc.to_vec2() * zoom;
                // Simple Bezier-like curve: two control points offset vertically.
                let ctrl0 = Pos2::new(p0.x, (p0.y + p1.y) / 2.0);
                let ctrl1 = Pos2::new(p1.x, (p0.y + p1.y) / 2.0);
                painter.line_segment([p0, ctrl0], edge_stroke);
                painter.line_segment([ctrl0, ctrl1], edge_stroke);
                painter.line_segment([ctrl1, p1], edge_stroke);
                // Arrowhead.
                let dir = (p1 - ctrl1).normalized();
                let perp = Vec2::new(-dir.y, dir.x);
                let tip = p1;
                let base0 = tip - dir * 8.0 * zoom + perp * 4.0 * zoom;
                let base1 = tip - dir * 8.0 * zoom - perp * 4.0 * zoom;
                painter.line_segment([tip, base0], edge_stroke);
                painter.line_segment([tip, base1], edge_stroke);
            }
        }

        // Draw nodes.
        let visuals = painter.ctx().global_style().visuals.clone();
        for nl in layout {
            let half_w = cfg.node_width / 2.0 * zoom;
            let half_h = cfg.node_height / 2.0 * zoom;
            let centre = origin + nl.centre.to_vec2() * zoom;
            let rect = Rect::from_center_size(centre, Vec2::new(half_w * 2.0, half_h * 2.0));
            painter.rect(
                rect,
                4.0 * zoom,
                visuals.widgets.inactive.bg_fill,
                Stroke::new(1.0 * zoom, visuals.widgets.inactive.bg_stroke.color),
                StrokeKind::Inside,
            );
            let label = self
                .labels
                .get(&nl.id)
                .cloned()
                .unwrap_or_else(|| format!("{}", nl.id.0));
            painter.text(
                centre,
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(12.0 * zoom),
                visuals.text_color(),
            );
        }
    }
}

impl Widget for &mut DagView {
    fn ui(self, ui: &mut Ui) -> Response {
        // Allocate all available space.
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size(),
            Sense::click_and_drag(),
        );

        // Handle panning.
        if response.dragged() {
            self.pan += response.drag_delta();
        }

        // Handle zoom via scroll.
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_delta != 0.0 {
            let factor = 1.0 + scroll_delta * 0.001;
            self.zoom = (self.zoom * factor).clamp(0.1, 10.0);
        }

        // Compute layout (cached).
        // We need to call compute_layout before borrowing painter, so we clone the layout.
        let layout: Vec<NodeLayout> = self.compute_layout().to_vec();

        // Draw.
        let painter = ui.painter_at(rect);
        let origin = rect.min + self.pan;
        self.draw(&painter, origin, &layout);

        response
    }
}
