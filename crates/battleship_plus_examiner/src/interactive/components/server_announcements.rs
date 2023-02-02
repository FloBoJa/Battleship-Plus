use std::cmp::max;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::ops::Add;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use chrono::{Duration, Utc};
use log::{debug, warn};
use quinn::VarInt;
use tuirealm::command::{Cmd, CmdResult, Position};
use tuirealm::event::{Key, KeyModifiers};
use tuirealm::props::{Color, Style};
use tuirealm::tui::layout::{Constraint, Rect};
use tuirealm::tui::widgets::{Block, Borders, Cell, Row, Table};
use tuirealm::{
    command, AttrValue, Attribute, Component, Event, Frame, MockComponent, NoUserEvent, Props,
    State, StateValue,
};

use battleship_plus_common::messages::ServerAdvertisement;
use bevy_quinnet_client::certificate::{CertificateVerificationMode, TrustOnFirstUseConfig};
use quin_client::client::QuicSocket;

use crate::advertisement_receiver::AdvertisementReceiver;
use crate::config::ADVERTISEMENT_PORT;
use crate::interactive::Message;

pub struct ServerAnnouncements {
    props: Props,
    quic_socket: QuicSocket,
    advertisement_receiver: AdvertisementReceiver,
    servers: HashSet<ServerEntry>,
    selected_server: Option<SocketAddr>,
    security_level: HashMap<SocketAddr, Arc<Mutex<Option<SecurityLevel>>>>,
}

impl Component<Message, NoUserEvent> for ServerAnnouncements {
    fn on(&mut self, event: Event<NoUserEvent>) -> Option<Message> {
        match event {
            Event::Tick => {
                if !matches!(self.perform(Cmd::Tick), CmdResult::None) {
                    Some(Message::Redraw)
                } else {
                    None
                }
            }
            Event::Keyboard(key_event) => match key_event.code {
                Key::Tab => {
                    if key_event.modifiers.contains(KeyModifiers::SHIFT) {
                        Some(Message::PreviousFocus)
                    } else {
                        Some(Message::NextFocus)
                    }
                }

                Key::Up => {
                    self.perform(Cmd::Move(command::Direction::Up));
                    Some(Message::Redraw)
                }
                Key::Down => {
                    self.perform(Cmd::Move(command::Direction::Down));
                    Some(Message::Redraw)
                }
                Key::Right | Key::Enter => {
                    if let CmdResult::Submit(State::One(StateValue::String(addr))) =
                        self.perform(Cmd::Submit)
                    {
                        match SocketAddr::from_str(addr.as_str()) {
                            Ok(addr) => return Some(Message::ConnectToServer(addr)),
                            Err(e) => {
                                warn!("unable to parse address \"{addr}\": {e}");
                            }
                        }
                    }

                    None
                }

                _ => None,
            },
            _ => None,
        }
    }
}

impl MockComponent for ServerAnnouncements {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        const INDICATOR: &str = " >> ";
        const INDICATOR_LENGTH: usize = INDICATOR.len();

        let mut selected_row = None;
        let mut addr_length = 1;
        let mut security_level_indicator_length = 1;
        let rows: Vec<_> = self
            .servers
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                addr_length = max(addr_length, entry.addr.to_string().len());

                let selected = self
                    .query(Attribute::Focus)
                    .unwrap_or(AttrValue::Flag(false))
                    .unwrap_flag()
                    && self.selected_server.is_some()
                    && self.selected_server.unwrap() == entry.addr;

                if selected {
                    selected_row = Some(i);
                }

                let (security_level, security_level_color) =
                    match self.security_level.get(&entry.addr) {
                        None => ("", Color::Reset),
                        Some(level) => level
                            .lock()
                            .expect("unable to acquire lock")
                            .map_or(("...", Color::Blue), |level| level.visual()),
                    };
                security_level_indicator_length = max(
                    security_level_indicator_length,
                    security_level.chars().count(),
                );

                Row::new(vec![
                    if selected {
                        Cell::from(INDICATOR)
                    } else {
                        Cell::default()
                    },
                    Cell::from(entry.addr.to_string()),
                    Cell::from(entry.display_name.as_str()),
                    Cell::from(security_level).style(Style::default().fg(security_level_color)),
                ])
                .style(if selected {
                    Style::default().bg(Color::Blue).fg(Color::LightYellow)
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

        addr_length += 2;
        let free_space = area.width as usize - 6 - INDICATOR_LENGTH;
        let name_length = free_space.saturating_sub(addr_length + security_level_indicator_length);

        let constraints = [
            Constraint::Length(INDICATOR_LENGTH as u16),
            Constraint::Length(addr_length as u16),
            Constraint::Length(name_length as u16),
            Constraint::Length(security_level_indicator_length as u16),
        ];

        let server_table = Table::new(rows.iter().skip(skip).take(visible_rows as usize).cloned())
            .block(Block::default().borders(Borders::NONE))
            .widths(&constraints);

        frame.render_widget(server_table, area);
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.props.get(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value)
    }

    fn state(&self) -> State {
        self.selected_server.map_or(State::None, |addr| {
            State::One(StateValue::String(addr.to_string()))
        })
    }

    fn perform(&mut self, cmd: Cmd) -> CmdResult {
        match cmd {
            Cmd::Tick => {
                let new_advertisements = self.poll_sockets();
                let mut changed = if !new_advertisements.is_empty() {
                    for advertisement in new_advertisements {
                        let entry: ServerEntry = advertisement.into();
                        self.servers.replace(entry.clone());
                        if self.security_level.contains_key(&entry.addr) {
                            continue;
                        }

                        let level_ref = Arc::new(Mutex::new(None));
                        self.security_level.insert(entry.addr, level_ref.clone());

                        let quic_socket = self.quic_socket.clone();

                        tokio::spawn(async move {
                            debug!("probe: {}", entry.addr);
                            let level = tokio::time::timeout(
                                std::time::Duration::from_secs(5),
                                Self::probe_server(quic_socket, entry.addr, None),
                            )
                            .await
                            .unwrap_or(SecurityLevel::TimedOut);
                            debug!("probe for {} finished -> {}", entry.addr, level);

                            let _ = level_ref
                                .lock()
                                .expect("unable to acquire lock")
                                .insert(level);
                        });
                    }

                    if self.selected_server.is_none() {
                        self.selected_server = self.servers.iter().next().map(|e| e.addr)
                    }

                    true
                } else {
                    false
                };

                self.servers
                    .clone()
                    .iter()
                    .filter(|e| e.valid_until < Utc::now())
                    .for_each(|e| {
                        self.servers.remove(e);
                        self.security_level.remove(&e.addr);
                        if self.selected_server == Some(e.addr) {
                            self.selected_server = None;
                        }
                        changed |= true;
                    });

                match changed {
                    true => CmdResult::Changed(self.state()),
                    false => CmdResult::None,
                }
            }

            Cmd::Scroll(direction) | Cmd::Move(direction) => {
                match (self.selected_server, direction) {
                    (None, _) => {
                        self.selected_server = self.servers.iter().next().map(|e| e.addr);
                        CmdResult::Changed(self.state())
                    }
                    (Some(selected), _) if !self.servers.iter().any(|e| e.addr == selected) => {
                        self.selected_server = self.servers.iter().next().map(|e| e.addr);
                        CmdResult::Changed(self.state())
                    }
                    (Some(selected), command::Direction::Down) => {
                        self.selected_server = self
                            .servers
                            .iter()
                            .cycle()
                            .skip_while(|e| e.addr != selected)
                            .nth(1)
                            .map(|e| e.addr);
                        CmdResult::Changed(self.state())
                    }
                    (Some(selected), command::Direction::Up) => {
                        self.selected_server = self
                            .servers
                            .iter()
                            .collect::<Vec<_>>()
                            .iter()
                            .rev()
                            .cycle()
                            .skip_while(|e| e.addr != selected)
                            .nth(1)
                            .map(|e| e.addr);
                        CmdResult::Changed(self.state())
                    }
                    (Some(_), command::Direction::Right) => self.perform(Cmd::Submit),
                    _ => CmdResult::None,
                }
            }
            Cmd::GoTo(position) => match position {
                Position::Begin => {
                    self.selected_server = self.servers.iter().next().map(|e| e.addr);
                    CmdResult::Changed(self.state())
                }
                Position::End => {
                    self.selected_server = self.servers.iter().last().map(|e| e.addr);
                    CmdResult::Changed(self.state())
                }
                Position::At(at) => {
                    self.selected_server = self.servers.iter().nth(at).map(|e| e.addr);
                    CmdResult::Changed(self.state())
                }
            },
            Cmd::Submit if self.selected_server.is_some() => CmdResult::Submit(self.state()),

            _ => CmdResult::None,
        }
    }
}

impl ServerAnnouncements {
    pub async fn new() -> Self {
        Self {
            quic_socket: QuicSocket::new().expect("unable to create new QuicSocket"),
            props: Props::default(),
            advertisement_receiver: AdvertisementReceiver::new(ADVERTISEMENT_PORT),
            servers: Default::default(),
            selected_server: None,
            security_level: Default::default(),
        }
    }

    fn poll_sockets(&mut self) -> Vec<(ServerAdvertisement, SocketAddr)> {
        self.advertisement_receiver.poll()
    }

    async fn probe_server(
        quick_socket: QuicSocket,
        addr: SocketAddr,
        hostname: Option<String>,
    ) -> SecurityLevel {
        let alpns = vec![
            battleship_plus_common::protocol_name_with_version(),
            battleship_plus_common::protocol_name(),
        ];

        let server_name = match hostname.as_ref() {
            None => addr.ip().to_string(),
            Some(s) => s.clone(),
        };

        let mut security = SecurityLevel::default();
        loop {
            if security.is_failed() {
                return SecurityLevel::ConnectionFailed;
            }

            let connection = match quick_socket.connect(
                security.get_verification_mode(),
                alpns.clone(),
                addr,
                server_name.clone(),
            ) {
                Ok(connecting) => connecting,
                Err(e) => {
                    debug!(
                        "failed to configure connection to {} with SecurityLevel::{security}: {e}",
                        server_name.as_str(),
                    );

                    security = security.next_weakest();
                    continue;
                }
            }
            .await;

            match connection {
                Ok(connection) => {
                    connection.close(VarInt::from_u32(0), &Vec::with_capacity(0));
                    return security;
                }
                Err(e) => {
                    debug!(
                        "failed to connect to {} with SecurityLevel::{security}: {e}",
                        server_name.as_str(),
                    );

                    security = security.next_weakest();
                    continue;
                }
            }
        }
    }
}

#[derive(Clone, PartialEq, Debug, Copy)]
pub enum SecurityLevel {
    ConnectionFailed,
    NoVerification,
    SelfSigned,
    AuthoritySigned,
    TimedOut,
}

impl Default for SecurityLevel {
    fn default() -> Self {
        SecurityLevel::AuthoritySigned
    }
}

impl Display for SecurityLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityLevel::ConnectionFailed => f.write_str("ConnectionFailed"),
            SecurityLevel::NoVerification => f.write_str("NoVerification"),
            SecurityLevel::SelfSigned => f.write_str("SelfSigned"),
            SecurityLevel::AuthoritySigned => f.write_str("AuthoritySigned"),
            SecurityLevel::TimedOut => f.write_str("TimedOut"),
        }
    }
}

impl SecurityLevel {
    pub(crate) fn is_failed(&self) -> bool {
        matches!(self, SecurityLevel::ConnectionFailed)
    }

    pub(crate) fn next_weakest(&self) -> SecurityLevel {
        match self {
            SecurityLevel::AuthoritySigned => SecurityLevel::SelfSigned,
            SecurityLevel::SelfSigned => SecurityLevel::NoVerification,
            SecurityLevel::NoVerification => SecurityLevel::ConnectionFailed,
            SecurityLevel::ConnectionFailed => SecurityLevel::ConnectionFailed,
            SecurityLevel::TimedOut => SecurityLevel::TimedOut,
        }
    }

    pub(crate) fn get_verification_mode(&self) -> CertificateVerificationMode {
        match self {
            SecurityLevel::AuthoritySigned => {
                CertificateVerificationMode::SignedByCertificateAuthority
            }
            SecurityLevel::SelfSigned => {
                CertificateVerificationMode::TrustOnFirstUse(TrustOnFirstUseConfig::default())
            }
            SecurityLevel::NoVerification => CertificateVerificationMode::SkipVerification,
            SecurityLevel::ConnectionFailed => CertificateVerificationMode::SkipVerification,
            SecurityLevel::TimedOut => CertificateVerificationMode::SkipVerification,
        }
    }

    pub(crate) fn visual(&self) -> (&'static str, Color) {
        match self {
            SecurityLevel::AuthoritySigned => ("\u{2713} (CA-signed)", Color::LightGreen),
            SecurityLevel::SelfSigned => ("\u{2713} (self-signed)", Color::Yellow),
            SecurityLevel::NoVerification => ("\u{2713} (unsigned)", Color::LightRed),
            SecurityLevel::ConnectionFailed => ("\u{2717} (failed)", Color::Red),
            SecurityLevel::TimedOut => ("\u{2717} (timeout)", Color::Red),
        }
    }
}

#[derive(Debug, Clone)]
struct ServerEntry {
    valid_until: chrono::DateTime<Utc>,
    addr: SocketAddr,
    display_name: String,
}

impl From<(ServerAdvertisement, SocketAddr)> for ServerEntry {
    fn from((advertisement, addr): (ServerAdvertisement, SocketAddr)) -> Self {
        let mut addr = addr;
        addr.set_port(advertisement.port as u16);

        Self {
            addr,
            display_name: advertisement.display_name,
            valid_until: Utc::now().add(Duration::seconds(10)),
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
