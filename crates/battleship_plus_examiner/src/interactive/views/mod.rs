use std::collections::HashSet;

use tuirealm::{Component, MockComponent};

use crate::interactive::views::server_selection::ServerSelectionIDs;

pub mod server_selection;

#[derive(Debug)]
pub enum Layout {
    ServerSelection(HashSet<ServerSelectionIDs>),
}
