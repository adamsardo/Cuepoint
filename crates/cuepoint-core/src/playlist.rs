use crate::TrackId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaylistKind {
    Folder,
    Playlist,
    /// Rule-based playlist. Its tracks are computed from the rules, which
    /// are kept as the source's raw definition until rule evaluation lands.
    Smart,
}

/// A node in the playlist tree: a folder holding children, or a playlist
/// holding track references in play order.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistNode {
    pub id: String,
    pub name: String,
    pub kind: PlaylistKind,
    pub children: Vec<PlaylistNode>,
    pub tracks: Vec<TrackId>,
    /// Raw rule definition for smart playlists.
    pub smart_rules: Option<String>,
}

impl PlaylistNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: PlaylistKind) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            children: Vec::new(),
            tracks: Vec::new(),
            smart_rules: None,
        }
    }

    /// Depth-first search for a node by id.
    pub fn find(&self, id: &str) -> Option<&PlaylistNode> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(id))
    }
}
