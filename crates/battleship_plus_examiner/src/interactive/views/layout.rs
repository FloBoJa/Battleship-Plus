use tuirealm::tui::layout::Rect;
use tuirealm::{Application, Frame, NoUserEvent};

use crate::interactive::views::server_selection;
use crate::interactive::views::server_selection::ServerSelectionID;
use crate::interactive::Message;

#[derive(Debug)]
pub enum Layout {
    ServerSelection(Vec<ServerSelectionID>),
}

impl Layout {
    pub fn draw(
        &self,
        app: &mut Application<i64, Message, NoUserEvent>,
        frame: &mut Frame,
        area: Rect,
    ) {
        match self {
            Layout::ServerSelection(ids) => server_selection::draw(ids, app, frame, area),
        }
    }

    pub fn server_selection(app: &mut Application<i64, Message, NoUserEvent>) -> Layout {
        server_selection::create(app)
    }
}
