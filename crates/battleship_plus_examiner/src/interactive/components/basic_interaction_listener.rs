use tuirealm::command::{Cmd, CmdResult};
use tuirealm::event::{Key, KeyEvent, KeyModifiers};
use tuirealm::tui::layout::Rect;
use tuirealm::{AttrValue, Attribute, Component, Event, Frame, MockComponent, NoUserEvent, State};

use crate::interactive::Msg;

pub struct BasicInteraction;

impl MockComponent for BasicInteraction {
    fn view(&mut self, _: &mut Frame, _: Rect) {}

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

impl Component<Msg, NoUserEvent> for BasicInteraction {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent {
                code: Key::Char('c'),
                modifiers: KeyModifiers::CONTROL,
            }) => Some(Msg::AppClose),
            Event::WindowResize(_, _) => Some(Msg::WindowResized),
            _ => unreachable!(),
        }
    }
}
