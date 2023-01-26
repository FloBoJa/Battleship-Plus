use std::net::SocketAddr;
use std::time::Duration;

use tui_logger::TuiLoggerWidget;
use tuirealm::event::{Key, KeyEvent, KeyModifiers};
use tuirealm::props::{Color, Style};
use tuirealm::terminal::TerminalBridge;
use tuirealm::tui::layout::{Constraint, Direction, Layout};
use tuirealm::tui::widgets::{Block, Borders};
use tuirealm::SubClause::Always;
use tuirealm::{
    event::NoUserEvent, Application, EventListenerCfg, Sub, SubClause, SubEventClause, Update,
};

use crate::interactive::components::basic_interaction_listener::BasicInteraction;
use crate::interactive::snowflake::snowflake_new_id;
use crate::interactive::views::server_selection::ServerSelectionView;

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

    current_view_id: i64,
}

impl Model {
    pub async fn new(addr: Option<SocketAddr>) -> Self {
        let (current_view_id, app) = Self::init_app(addr).await;

        Self {
            app,
            current_view_id,
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

                self.app.view(&self.current_view_id, f, body)
            })
            .is_ok());
    }

    async fn init_app(addr: Option<SocketAddr>) -> (i64, Application<i64, Message, NoUserEvent>) {
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

        let current_view_id: i64;
        // Mount components
        if let Some(addr) = addr {
            todo!()
        } else {
            let server_selection_view = ServerSelectionView::new().await;
            current_view_id = server_selection_view.id();
            app.mount(
                current_view_id,
                Box::new(server_selection_view),
                vec![Sub::new(
                    SubEventClause::Keyboard(KeyEvent::new(Key::Null, KeyModifiers::all())),
                    SubClause::IsMounted(current_view_id),
                )],
            )
            .expect("unable to mount ServerSelectionView");
        }

        app.active(&current_view_id)
            .expect("unable to focus the current view");
        (current_view_id, app)
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
                Message::WindowResized => None,
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
