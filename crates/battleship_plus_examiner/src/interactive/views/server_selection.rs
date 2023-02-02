use std::net::ToSocketAddrs;

use log::warn;
use tuirealm::props::{Alignment, Color, Style, TextModifiers};
use tuirealm::tui::layout::{Constraint, Direction, Margin, Rect};
use tuirealm::{tui, Application, Frame, NoUserEvent, Sub, SubClause, SubEventClause};

use crate::interactive::components::server_announcements::ServerAnnouncements;
use crate::interactive::components::server_selection_background::ServerSelectionBackground;
use crate::interactive::components::text_entry::TextBox;
use crate::interactive::snowflake::snowflake_new_id;
use crate::interactive::views::layout::Layout;
use crate::interactive::Message;

#[derive(Debug, Copy, Clone)]
pub struct ServerSelectionDrawAreas {
    pub direct_connect_box: Rect,
    pub direct_connect_text: Rect,
    pub direct_connect_input: Rect,
    pub server_list_box: Rect,
    pub server_list_box_inner: Rect,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ServerSelectionID {
    ServerSelectionBackground(i64),
    DirectConnectInput(i64),
    ServerList(i64),
}

impl ServerSelectionID {
    pub fn id(&self) -> i64 {
        match self {
            ServerSelectionID::ServerSelectionBackground(id)
            | ServerSelectionID::DirectConnectInput(id)
            | ServerSelectionID::ServerList(id) => *id,
        }
    }
}

pub(crate) fn draw_areas(area: Rect) -> ServerSelectionDrawAreas {
    let vertical_layout = tui::layout::Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(4), Constraint::Min(1)])
        .split(area);

    let direct_connect_vertical_layout = tui::layout::Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(1), Constraint::Length(1)])
        .split(area.inner(&Margin {
            vertical: 1,
            horizontal: 3,
        }));

    ServerSelectionDrawAreas {
        direct_connect_box: vertical_layout[0],
        direct_connect_text: direct_connect_vertical_layout[0],
        direct_connect_input: direct_connect_vertical_layout[1],
        server_list_box: vertical_layout[1],
        server_list_box_inner: vertical_layout[1].inner(&Margin {
            vertical: 1,
            horizontal: 1,
        }),
    }
}

pub(crate) async fn create(app: &mut Application<i64, Message, NoUserEvent>) -> Layout {
    let background = (snowflake_new_id(), ServerSelectionBackground);
    app.mount(background.0, Box::new(background.1), vec![])
        .expect("unable to mount background");

    let address_entry = (
        snowflake_new_id(),
        TextBox::with_text(
            "bsplus.floboja.net:30305",
            Box::new(|_| true),
            Some(Box::new(|text| {
                let msg = text.to_socket_addrs().map_or(None, |mut addr| {
                    addr.clone()
                        .find(|a| a.is_ipv6())
                        .or(addr.next())
                        .map(Message::ConnectToServer)
                });

                if msg.is_none() {
                    warn!("{text} seems to be invalid")
                }

                msg
            })),
        )
        .align(Alignment::Center)
        .background_style(Style::default().bg(Color::DarkGray))
        .foreground_style(Style::default().bg(Color::DarkGray).fg(Color::White))
        .cursor_style(
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(TextModifiers::UNDERLINED),
        ),
    );
    app.mount(address_entry.0, Box::new(address_entry.1), vec![])
        .expect("unable to mount address_entry");

    let server_list = (snowflake_new_id(), ServerAnnouncements::new().await);
    app.mount(
        server_list.0,
        Box::new(server_list.1),
        vec![Sub::new(
            SubEventClause::Tick,
            SubClause::IsMounted(server_list.0),
        )],
    )
    .expect("unable to mount server_list");

    app.active(&address_entry.0)
        .expect("unable to focus address_entry");

    Layout::ServerSelection {
        focus_rotation: vec![
            ServerSelectionID::DirectConnectInput(address_entry.0),
            ServerSelectionID::ServerList(server_list.0),
        ],
        ids: vec![
            ServerSelectionID::ServerSelectionBackground(background.0),
            ServerSelectionID::DirectConnectInput(address_entry.0),
            ServerSelectionID::ServerList(server_list.0),
        ],
    }
}

pub(crate) fn draw(
    ids: &[ServerSelectionID],
    app: &mut Application<i64, Message, NoUserEvent>,
    frame: &mut Frame,
    area: Rect,
) {
    let draw_areas: ServerSelectionDrawAreas = draw_areas(area);

    for id in ids {
        match id {
            ServerSelectionID::ServerSelectionBackground(id) => app.view(id, frame, area),
            ServerSelectionID::DirectConnectInput(id) => {
                app.view(id, frame, draw_areas.direct_connect_input)
            }
            ServerSelectionID::ServerList(id) => {
                app.view(id, frame, draw_areas.server_list_box_inner)
            }
        }
    }
}
