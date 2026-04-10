//! An [`egui`] [`Widget`](egui::Widget) that renders the commit history of a local git repository
//! as an interactive DAG (directed acyclic graph).
//!
//! Internally this crate uses [`egui_dag_view::DagView`] to handle the generic graph layout and
//! interaction; it is responsible only for loading git commit data via [`git2`] and translating it
//! into the form that [`DagView`] expects.
//!
//! # Example
//!
//! ```no_run
//! use egit_repo_view::RepoView;
//! use std::path::Path;
//!
//! let mut view = RepoView::open(Path::new(".")).expect("failed to open repo");
//! // In your egui update loop:
//! // ui.add(&mut view);
//! ```

use egui::{Response, Ui, Widget};
use egui_dag_view::{DagView, NodeId};
use git2::{Oid, Repository};
use std::collections::HashMap;
use std::path::Path;

/// An [`egui`] widget that renders the commit history of a git repository as an interactive DAG.
///
/// Each commit is a node; parent relationships form the edges.  The widget delegates rendering
/// to [`egui_dag_view::DagView`].
pub struct RepoView {
    dag: DagView,
}

impl RepoView {
    /// Open the repository at `path` and build the DAG view from its commit history.
    ///
    /// This walks all reachable commits starting from all references and builds a commit-parent
    /// DAG.  The commit's short hash (first 7 hex chars) is used as the node label.
    ///
    /// # Errors
    ///
    /// Returns a [`git2::Error`] if the repository cannot be opened or the commit walk fails.
    pub fn open(path: &Path) -> Result<Self, git2::Error> {
        let repo = Repository::open(path)?;
        let (nodes, edges, labels) = build_dag(&repo)?;
        let mut dag = DagView::new(nodes, edges);
        for (id, label) in labels {
            dag = dag.with_label(id, label);
        }
        Ok(Self { dag })
    }
}

impl Widget for &mut RepoView {
    fn ui(self, ui: &mut Ui) -> Response {
        ui.add(&mut self.dag)
    }
}

// ── DAG construction ─────────────────────────────────────────────────────────

/// DAG construction result: nodes, edges, labels.
type DagData = (Vec<NodeId>, Vec<(NodeId, NodeId)>, HashMap<NodeId, String>);

/// Walk the repository and return `(nodes, edges, labels)` suitable for [`DagView::new`].
fn build_dag(repo: &Repository) -> Result<DagData, git2::Error> {
    let mut revwalk = repo.revwalk()?;
    revwalk.push_glob("*")?;
    revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;

    let mut nodes: Vec<NodeId> = Vec::new();
    let mut edges: Vec<(NodeId, NodeId)> = Vec::new();
    let mut labels: HashMap<NodeId, String> = HashMap::new();
    let mut seen: HashMap<Oid, NodeId> = HashMap::new();

    let oid_to_node = |oid: Oid| -> NodeId {
        // Fold the 20-byte SHA-1 into a u64 by XOR-folding.
        let bytes = oid.as_bytes();
        let mut val = 0u64;
        for chunk in bytes.chunks(8) {
            let mut buf = [0u8; 8];
            buf[..chunk.len()].copy_from_slice(chunk);
            val ^= u64::from_le_bytes(buf);
        }
        NodeId(val)
    };

    for oid_result in revwalk {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;
        let node_id = oid_to_node(oid);

        if seen.insert(oid, node_id).is_none() {
            nodes.push(node_id);
            // Short commit hash as label.
            let short = &oid.to_string()[..7];
            labels.insert(node_id, short.to_owned());
        }

        for parent in commit.parents() {
            let parent_oid = parent.id();
            let parent_node = oid_to_node(parent_oid);
            if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(parent_oid) {
                e.insert(parent_node);
                nodes.push(parent_node);
                let short = &parent_oid.to_string()[..7];
                labels.insert(parent_node, short.to_owned());
            }
            // Edge direction: parent → child (parent is a predecessor in the DAG).
            edges.push((parent_node, node_id));
        }
    }

    Ok((nodes, edges, labels))
}
