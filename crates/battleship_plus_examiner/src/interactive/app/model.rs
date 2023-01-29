use std::net::SocketAddr;
use std::time::Duration;

use log::debug;
use tui_logger::TuiLoggerWidget;
use tuirealm::event::{Key, KeyEvent, KeyModifiers};
use tuirealm::props::{Color, Style};
use tuirealm::terminal::TerminalBridge;
use tuirealm::tui::layout::{Constraint, Direction, Layout};
use tuirealm::tui::widgets::{Block, Borders};
use tuirealm::SubClause::Always;
use tuirealm::{event::NoUserEvent, Application, EventListenerCfg, Sub, SubEventClause, Update};

use crate::interactive::components::basic_interaction_listener::BasicInteraction;
use crate::interactive::snowflake::snowflake_new_id;

use super::Message;

pub struct Model {
    /// Application
    pub app: Application<i64, Message, NoUserEvent>,
    /// Indicates that the application must quit
    pub quit: bool,
    /// Tells whether to redraw interface
    pub redraw: bool,
    /// Used to draw to terminal
    pub terminal: TerminalBridge,

    current_layout: crate::interactive::views::layout::Layout,
}

impl Model {
    pub async fn new(addr: Option<SocketAddr>) -> Self {
        let (current_layout, app) = Self::init_app(addr).await;

        Self {
            app,
            current_layout,
            quit: false,
            redraw: true,
            terminal: TerminalBridge::new().expect("Cannot initialize terminal"),
        }
    }

    pub fn view(&mut self) {
        assert!(self
            .terminal
            .raw_mut()
            .draw(|f| {
                let layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(vec![Constraint::Percentage(75), Constraint::Percentage(25)])
                    .split(f.size());

                f.render_widget(draw_logs(), layout[1]);

                let body = layout[0];
                self.current_layout.draw(&mut self.app, f, body);
            })
            .is_ok());
    }

    async fn init_app(
        addr: Option<SocketAddr>,
    ) -> (
        crate::interactive::views::layout::Layout,
        Application<i64, Message, NoUserEvent>,
    ) {
        let mut app: Application<i64, Message, NoUserEvent> = Application::init(
            EventListenerCfg::default()
                .default_input_listener(Duration::from_millis(20))
                .poll_timeout(Duration::from_millis(10))
                .tick_interval(Duration::from_secs(1)),
        );

        let resize_listener_id = snowflake_new_id();
        app.mount(
            resize_listener_id,
            Box::new(BasicInteraction),
            vec![
                Sub::new(SubEventClause::WindowResize, Always),
                Sub::new(
                    SubEventClause::Keyboard(KeyEvent {
                        code: Key::Char('c'),
                        modifiers: KeyModifiers::CONTROL,
                    }),
                    Always,
                ),
            ],
        )
        .expect("unable to mount resize listener");

        let layout;
        // Mount components
        if let Some(addr) = addr {
            todo!()
        } else {
            layout = crate::interactive::views::layout::Layout::server_selection(&mut app).await;
        }

        (layout, app)
    }
}

// Let's implement Update for model

impl Update<Message> for Model {
    fn update(&mut self, msg: Option<Message>) -> Option<Message> {
        if let Some(msg) = msg {
            // Set redraw
            self.redraw = true;
            // Match message

            match msg {
                Message::AppClose => {
                    self.quit = true;
                    None
                }
                Message::Redraw => None,
                Message::NextFocus => {
                    if let Some(id) = self.current_layout.next_focus() {
                        self.app.blur().expect("unable to take focus from element");
                        self.app.active(&id).expect("unable to focus {id}");
                    }
                    None
                }
                Message::PreviousFocus => {
                    if let Some(id) = self.current_layout.previous_focus() {
                        self.app.blur().expect("unable to take focus from element");
                        self.app.active(&id).expect("unable to focus {id}");
                    }
                    None
                }
                Message::ConnectToServer(addr) => {
                    debug!("Connecting to server {addr}…");
                    None
                }
                _ => Some(msg),
            }
        } else {
            None
        }
    }
}

fn draw_logs<'a>() -> TuiLoggerWidget<'a> {
    TuiLoggerWidget::default()
        .style_error(Style::default().fg(Color::Red))
        .style_debug(Style::default().fg(Color::Green))
        .style_warn(Style::default().fg(Color::Yellow))
        .style_trace(Style::default().fg(Color::Gray))
        .style_info(Style::default().fg(Color::Blue))
        .block(Block::default().title("[ Logs ]").borders(Borders::ALL))
}
