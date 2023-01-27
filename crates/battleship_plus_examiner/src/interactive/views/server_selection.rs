use once_cell::sync::Lazy;
use tuirealm::props::{Alignment, Color, Style, TextModifiers};
use tuirealm::tui::layout::{Constraint, Direction, Margin, Rect};
use tuirealm::{tui, Application, Frame, NoUserEvent};

use crate::interactive::components::server_selection_background::ServerSelectionBackground;
use crate::interactive::components::text_entry::TextBox;
use crate::interactive::snowflake::snowflake_new_id;
use crate::interactive::views::layout::Layout;
use crate::interactive::Message;

#[derive(Debug, Copy, Clone)]
pub struct ServerSelectionDrawAreas {
    pub direct_connect_box: Rect,
    pub direct_connect_text: Rect,
    pub direct_connect_form_addr: Rect,
    pub direct_connect_form_separator: Rect,
    pub direct_connect_form_port: Rect,
    pub direct_connect_form_button: Rect,
    pub server_list_box: Rect,
    pub server_list_box_inner: Rect,
}

const CONNECT: &str = "[ CONNECT ]";
const CONNECT_LENGTH: usize = CONNECT.len();
static PORT_ENTRY_LENGTH: Lazy<usize> = Lazy::new(|| u16::MAX.to_string().len());

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ServerSelectionID {
    ServerSelectionBackground(i64),
    DirectConnectAddress(i64),
    AddressPortSeparator(i64),
    DirectConnectPort(i64),
    DirectConnectButton(i64),
    ServerList(i64),
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

    let direct_connect_form = tui::layout::Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![
            Constraint::Min(8),
            Constraint::Length(3),
            Constraint::Length(*PORT_ENTRY_LENGTH as u16),
            Constraint::Length(1),
            Constraint::Length(CONNECT_LENGTH as u16),
        ])
        .split(direct_connect_vertical_layout[1]);

    ServerSelectionDrawAreas {
        direct_connect_box: vertical_layout[0],
        direct_connect_text: direct_connect_vertical_layout[0],
        direct_connect_form_addr: direct_connect_form[0],
        direct_connect_form_separator: direct_connect_form[1],
        direct_connect_form_port: direct_connect_form[2],
        direct_connect_form_button: direct_connect_form[4],
        server_list_box: vertical_layout[1],
        server_list_box_inner: vertical_layout[1].inner(&Margin {
            vertical: 1,
            horizontal: 3,
        }),
    }
}

pub(crate) fn create(app: &mut Application<i64, Message, NoUserEvent>) -> Layout {
    let background = (snowflake_new_id(), ServerSelectionBackground);
    app.mount(background.0, Box::new(background.1), vec![])
        .expect("unable to mount background");

    let address_entry = (
        snowflake_new_id(),
        TextBox::with_text("bsplus.floboja.net", Box::new(|_| true), None)
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

    let address_port_separator = (
        snowflake_new_id(),
        TextBox::label(":").align(Alignment::Center),
    );
    app.mount(
        address_port_separator.0,
        Box::new(address_port_separator.1),
        vec![],
    )
    .expect("unable to mount address_port_separator");

    let port_entry = (
        snowflake_new_id(),
        TextBox::with_text("30305", Box::new(|c| c.is_ascii_digit()), None)
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
    app.mount(port_entry.0, Box::new(port_entry.1), vec![])
        .expect("unable to mount port_entry");

    let connect_button = (
        snowflake_new_id(),
        TextBox::button(CONNECT, Box::new(|_| todo!()))
            .align(Alignment::Center)
            .foreground_style(Style::default().bg(Color::Yellow).fg(Color::Black))
            .cursor_style(Style::default().bg(Color::Yellow).fg(Color::Black)),
    );
    app.mount(connect_button.0, Box::new(connect_button.1), vec![])
        .expect("unable to mount connect_button");

    Layout::ServerSelection(vec![
        ServerSelectionID::ServerSelectionBackground(background.0),
        ServerSelectionID::DirectConnectAddress(address_entry.0),
        ServerSelectionID::AddressPortSeparator(address_port_separator.0),
        ServerSelectionID::DirectConnectPort(port_entry.0),
        ServerSelectionID::DirectConnectButton(connect_button.0),
    ])
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
            ServerSelectionID::ServerSelectionBackground(id) => app.view(&id, frame, area),
            ServerSelectionID::DirectConnectAddress(id) => {
                app.view(&id, frame, draw_areas.direct_connect_form_addr)
            }
            ServerSelectionID::AddressPortSeparator(id) => {
                app.view(&id, frame, draw_areas.direct_connect_form_separator)
            }
            ServerSelectionID::DirectConnectPort(id) => {
                app.view(&id, frame, draw_areas.direct_connect_form_port)
            }
            ServerSelectionID::DirectConnectButton(id) => {
                app.view(&id, frame, draw_areas.direct_connect_form_button)
            }
            ServerSelectionID::ServerList(id) => app.view(&id, frame, draw_areas.server_list_box),
        }
    }
}
