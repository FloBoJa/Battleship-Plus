use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::{Alignment, Color, Style};
use tuirealm::tui::layout::Rect;
use tuirealm::tui::style::Modifier;
use tuirealm::tui::text::{Span, Spans, Text};
use tuirealm::tui::widgets::{Block, Borders, Paragraph};
use tuirealm::{AttrValue, Attribute, Component, Event, Frame, MockComponent, NoUserEvent, State};

use crate::interactive::views::server_selection;
use crate::interactive::views::server_selection::ServerSelectionDrawAreas;
use crate::interactive::Message;

#[derive(Default, Copy, Clone)]
pub struct ServerSelectionBackground;

impl MockComponent for ServerSelectionBackground {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let box_style = Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD);

        let draw_areas: ServerSelectionDrawAreas = server_selection::draw_areas(area);

        let direct_connect_box = Block::default()
            .borders(Borders::ALL)
            .title("[ Direct Connect ]")
            .border_style(box_style);

        let direct_connect_text = Paragraph::new(Text::from(vec![Spans::from(Span::styled(
            "Connect to a server via its address",
            Style::default().add_modifier(Modifier::BOLD),
        ))]))
        .alignment(Alignment::Left);

        let server_list_box = Block::default()
            .borders(Borders::ALL)
            .title("[ Server List ]")
            .border_style(box_style);

        frame.render_widget(direct_connect_box, draw_areas.direct_connect_box);
        frame.render_widget(direct_connect_text, draw_areas.direct_connect_text);
        frame.render_widget(server_list_box, draw_areas.server_list_box);
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

impl Component<Message, NoUserEvent> for ServerSelectionBackground {
    fn on(&mut self, _: Event<NoUserEvent>) -> Option<Message> {
        None
    }
}
