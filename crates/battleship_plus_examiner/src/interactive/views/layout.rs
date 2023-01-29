use tuirealm::tui::layout::Rect;
use tuirealm::{Application, Frame, NoUserEvent};

use crate::interactive::views::server_selection;
use crate::interactive::views::server_selection::ServerSelectionID;
use crate::interactive::Message;

#[derive(Debug)]
pub enum Layout {
    ServerSelection {
        focus_rotation: Vec<ServerSelectionID>,
        ids: Vec<ServerSelectionID>,
    },
}

impl Layout {
    pub fn draw(
        &self,
        app: &mut Application<i64, Message, NoUserEvent>,
        frame: &mut Frame,
        area: Rect,
    ) {
        match self {
            Layout::ServerSelection { ids, .. } => server_selection::draw(ids, app, frame, area),
        }
    }

    pub fn next_focus(&mut self) -> Option<i64> {
        match self {
            Layout::ServerSelection { focus_rotation, .. } => {
                if focus_rotation.is_empty() {
                    return None;
                }

                let id = focus_rotation.remove(0);
                focus_rotation.push(id);
                focus_rotation.first().map(|id| id.id())
            }
        }
    }

    pub fn previous_focus(&mut self) -> Option<i64> {
        match self {
            Layout::ServerSelection { focus_rotation, .. } => {
                if let Some(id) = focus_rotation.pop() {
                    focus_rotation.insert(0, id);
                    focus_rotation.first().map(|id| id.id())
                } else {
                    None
                }
            }
        }
    }

    pub async fn server_selection(app: &mut Application<i64, Message, NoUserEvent>) -> Layout {
        server_selection::create(app).await
    }
}
