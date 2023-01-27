use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::ops::Add;

use chrono::{Duration, Utc};
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::{Alignment, Style};
use tuirealm::tui::layout::{Constraint, Direction, Layout, Margin, Rect};
use tuirealm::tui::style::Modifier;
use tuirealm::tui::text::{Span, Spans, Text};
use tuirealm::tui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use tuirealm::{AttrValue, Attribute, Component, Event, Frame, MockComponent, NoUserEvent, State};

use battleship_plus_common::messages::ServerAdvertisement;
use battleship_plus_common::types::Config;

use crate::advertisement_receiver::AdvertisementReceiver;
use crate::config::ADVERTISEMENT_PORT;
use crate::interactive::snowflake::snowflake_new_id;
use crate::interactive::styles::styles;
use crate::interactive::Message;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ServerSelectionIDs {
    DirectConnectAddress(i64),
    DirectConnectPort(i64),
    DirectConnectButton(i64),
    ServerList(i64),
}

#[derive(Debug, Clone)]
struct ServerEntry {
    valid_until: chrono::DateTime<Utc>,
    addr: SocketAddr,
    display_name: String,
    game_config: Option<Config>,
}

impl From<(ServerAdvertisement, SocketAddr)> for ServerEntry {
    fn from((advertisement, addr): (ServerAdvertisement, SocketAddr)) -> Self {
        let addr = SocketAddr::new(addr.ip(), advertisement.port as u16);

        Self {
            addr,
            display_name: advertisement.display_name,
            valid_until: Utc::now().add(Duration::seconds(30)),
            game_config: None,
        }
    }
}

impl Eq for ServerEntry {}

impl Hash for ServerEntry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.addr.hash(state)
    }
}

impl PartialEq for ServerEntry {
    fn eq(&self, other: &Self) -> bool {
        self.addr.eq(&other.addr)
    }
}

#[derive(Debug)]
pub struct ServerSelectionView {
    ui_id: i64,
    advertisement_receiver: AdvertisementReceiver,
    servers: HashSet<ServerEntry>,
    selected_server: Option<SocketAddr>,

    direct_connect_addr: String,
    direct_connect_port: String,
}

impl Component<Message, NoUserEvent> for ServerSelectionView {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Message> {
        match ev {
            Event::Tick => {
                let new_advertisements = self.poll_sockets();
                if !new_advertisements.is_empty() {
                    for advertisement in new_advertisements {
                        let entry = advertisement.into();
                        self.servers.replace(entry);
                    }

                    return Some(Message::ServerSelectionMessage(
                        SeverSelectionMessage::StateChanged,
                    ));
                }

                None
            }
            _ => None,
        }
    }
}

impl MockComponent for ServerSelectionView {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(4), Constraint::Min(1)])
            .split(area);

        self.draw_direct_connect(frame, layout[0]);
        self.draw_server_list(frame, layout[1]);
    }

    fn query(&self, _: Attribute) -> Option<AttrValue> {
        None
    }

    fn attr(&mut self, _: Attribute, _: AttrValue) {}

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl ServerSelectionView {
    pub async fn new() -> Self {
        Self {
            ui_id: snowflake_new_id(),
            advertisement_receiver: AdvertisementReceiver::new(ADVERTISEMENT_PORT),
            servers: Default::default(),
            selected_server: None,
            direct_connect_addr: "bsplus.floboja.net".to_string(),
            direct_connect_port: "30305".to_string(),
        }
    }

    pub fn id(&self) -> i64 {
        self.ui_id
    }

    fn draw_direct_connect(&self, f: &mut Frame, area: Rect) {
        let direct_connect_paragraph_size = area.inner(&Margin {
            vertical: 1,
            horizontal: 3,
        });

        const PORT_ENTRY_WIDTH: usize = 7;

        let direct_connect_box = Block::default()
            .borders(Borders::ALL)
            .title("[ Direct Connect ]")
            .border_style(*styles::not_focus::BOX);

        let address_entry_styles = (*styles::focus::TEXT, *styles::focus::TEXT_PADDING);

        //let address_entry = padded_span(
        //    self.direct_connect_addr.as_str(),
        //    Alignment::Center,
        //    direct_connect_paragraph_size.width as usize
        //        - PORT_ENTRY_WIDTH
        //        - 2
        //        - CONNECT_BUTTON_TEXT.len()
        //        - 2,
        //    address_entry_styles.1,
        //    address_entry_styles.0,
        //);

        let port_entry_styles = (*styles::focus::TEXT, *styles::focus::TEXT_PADDING);

        //let port_entry = padded_span(
        //    self.direct_connect_port.as_str(),
        //    Alignment::Center,
        //    PORT_ENTRY_WIDTH,
        //    port_entry_styles.1,
        //    port_entry_styles.0,
        //);

        let connect_button_style = *styles::focus::TEXT;

        let connect_button = connect_button(connect_button_style);

        let direct_connect_paragraph = Paragraph::new(Text::from(vec![
            Spans::from(Span::styled(
                "Connect to a server via its address",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Spans::from(
                vec![
                    //address_entry.0,
                    vec![Span::from(":")],
                    //port_entry.0,
                    vec![Span::from(" ")],
                    vec![connect_button],
                ]
                .iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>(),
            ),
        ]))
        .alignment(Alignment::Left);

        f.render_widget(direct_connect_box, area);
        f.render_widget(direct_connect_paragraph, direct_connect_paragraph_size);
    }

    fn draw_server_list(&self, f: &mut Frame, area: Rect) {
        let server_list_box = Block::default()
            .borders(Borders::ALL)
            .title("[ Server List ]")
            .border_style(*styles::focus::BOX);

        const INDICATOR: &str = " >> ";
        const INDICATOR_LENGTH: usize = INDICATOR.len();

        let mut selected_row = None;
        let rows: Vec<_> = self
            .servers
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let selected =
                    self.selected_server.is_some() && self.selected_server.unwrap() == entry.addr;

                if selected {
                    selected_row = Some(i);
                }

                Row::new(vec![
                    if selected {
                        Cell::from(INDICATOR)
                    } else {
                        Cell::default()
                    },
                    Cell::from(entry.addr.to_string()),
                    Cell::from(entry.display_name.as_str()),
                    if selected {
                        Cell::from(connect_button(Style::default()))
                    } else {
                        Cell::default()
                    },
                ])
                .style(if selected {
                    *styles::focus::TEXT
                } else {
                    Style::default()
                })
            })
            .collect();

        let visible_rows = area.height - 2;
        let mut skip = 0;
        if let Some(selected_row) = selected_row {
            if selected_row > (visible_rows - 2) as usize {
                skip = selected_row - (visible_rows - 2) as usize;
            }
        }

        let free_space = area.width as usize - 6 - INDICATOR_LENGTH - CONNECT_BUTTON_TEXT_LENGTH;
        let addr_length = free_space / 3;
        let name_length = free_space - addr_length;

        let constraints = [
            Constraint::Length(INDICATOR_LENGTH as u16),
            Constraint::Length(addr_length as u16),
            Constraint::Length(name_length as u16),
            Constraint::Length(CONNECT_BUTTON_TEXT_LENGTH as u16),
        ];

        let server_table = Table::new(rows.iter().skip(skip).take(visible_rows as usize).cloned())
            .block(server_list_box)
            .widths(&constraints);

        f.render_widget(server_table, area);
    }

    fn poll_sockets(&mut self) -> Vec<(ServerAdvertisement, SocketAddr)> {
        self.advertisement_receiver.poll()
    }
}

const CONNECT_BUTTON_TEXT: &str = "[ Connect ]";
const CONNECT_BUTTON_TEXT_LENGTH: usize = CONNECT_BUTTON_TEXT.len();

fn connect_button<'a>(style: Style) -> Span<'a> {
    Span::styled(CONNECT_BUTTON_TEXT, style)
}

#[derive(Debug, PartialEq)]
pub enum SeverSelectionMessage {
    StateChanged,
}
