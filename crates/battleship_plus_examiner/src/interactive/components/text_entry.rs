use std::cmp::{max, min};
use std::collections::HashMap;
use std::string::ToString;

use tuirealm::command::{Cmd, CmdResult, Direction, Position};
use tuirealm::event::{Key, KeyModifiers};
use tuirealm::props::{Alignment, PropPayload, PropValue, Style};
use tuirealm::tui::layout::Rect;
use tuirealm::tui::text::{Span, Spans, Text};
use tuirealm::tui::widgets::Paragraph;
use tuirealm::{
    AttrValue, Attribute, Component, Event, Frame, MockComponent, NoUserEvent, Props, State,
    StateValue,
};

use crate::interactive::Message;

pub struct TextEntry {
    props: Props,
    char_validator: Box<dyn Fn(char) -> bool>,
    value_validator: Box<dyn Fn(&str) -> bool>,
}

const TEXT: &str = "TEXT";
const CURSOR: &str = "CURSOR";

impl MockComponent for TextEntry {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let state = self.state().unwrap_map();
        let state_string = state.get(TEXT).unwrap().clone().unwrap_string();
        let state_cursor = state.get(CURSOR).unwrap().clone().unwrap_u32();

        let width = area.width as usize;
        let (state_string, state_cursor) = if state_string.chars().count() > width {
            let end = state_string.chars().count();
            let begin = if state_cursor < (end - width) as u32 {
                state_cursor as usize
            } else {
                end - width
            };

            (
                state_string
                    .as_str()
                    .char_indices()
                    .map(|(_, c)| c)
                    .skip(begin)
                    .take(width)
                    .collect::<String>(),
                state_cursor - begin as u32,
            )
        } else {
            (state_string, state_cursor)
        };

        let align = self
            .query(Attribute::TextAlign)
            .unwrap_or(AttrValue::Alignment(Alignment::Left))
            .unwrap_alignment();

        let padding_style = match self.query(Attribute::Background) {
            None => Style::default(),
            Some(style) => style.unwrap_style(),
        };
        let content_style = match self.query(Attribute::Foreground) {
            None => Style::default(),
            Some(style) => style.unwrap_style(),
        };
        let cursor_style = match self.query(Attribute::HighlightedColor) {
            None => Style::default(),
            Some(style) => style.unwrap_style(),
        };

        let pad_left = match align {
            Alignment::Left => 0,
            Alignment::Center => (width - state_string.chars().count()) / 2,
            Alignment::Right => width - state_string.chars().count(),
        };
        let mut pad_right = match align {
            Alignment::Left => width - state_string.chars().count(),
            Alignment::Center => (width - state_string.chars().count()) / 2,
            Alignment::Right => 0,
        };
        if pad_left + pad_right + state_string.chars().count() < width {
            pad_right = width - (pad_left + state_string.chars().count());
        }

        let spans = Spans::from(
            vec![
                if pad_left > 0 {
                    Some(Span::styled(repeat(' ', pad_left), padding_style))
                } else {
                    None
                },
                if state_cursor > 0 {
                    Some(Span::styled(
                        &state_string.as_str()[..state_string
                            .as_str()
                            .char_indices()
                            .nth(state_cursor as usize)
                            .unwrap()
                            .0],
                        content_style,
                    ))
                } else {
                    None
                },
                if state_string.is_empty() {
                    Some(Span::styled("_", cursor_style))
                } else {
                    Some(Span::styled(
                        state_string
                            .as_str()
                            .char_indices()
                            .nth(state_cursor as usize)
                            .unwrap()
                            .1
                            .to_string(),
                        cursor_style,
                    ))
                },
                if state_cursor + 1 < state_string.chars().count() as u32 {
                    Some(Span::styled(
                        &state_string.as_str()[state_string
                            .chars()
                            .as_str()
                            .char_indices()
                            .nth(state_cursor as usize + 1)
                            .unwrap()
                            .0..],
                        content_style,
                    ))
                } else {
                    None
                },
                if pad_right > 0 {
                    Some(Span::styled(repeat(' ', pad_right), padding_style))
                } else {
                    None
                },
            ]
            .iter_mut()
            .filter_map(|span| span.take())
            .collect::<Vec<_>>(),
        );

        frame.render_widget(Paragraph::new(Text::from(spans)), area);
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.props.get(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value)
    }

    fn state(&self) -> State {
        State::Map(HashMap::from([
            (
                TEXT.to_string(),
                StateValue::String(match self.query(Attribute::Text) {
                    None => String::new(),
                    Some(text) => text.unwrap_string(),
                }),
            ),
            (
                CURSOR.to_string(),
                StateValue::U32(match self.query(Attribute::Custom(CURSOR)) {
                    None => 0u32,
                    Some(cursor) => cursor.unwrap_payload().unwrap_one().unwrap_u32(),
                }),
            ),
        ]))
    }

    fn perform(&mut self, cmd: Cmd) -> CmdResult {
        match cmd {
            Cmd::Type(c) => {
                if (self.char_validator)(c) {
                    let state = self.state().unwrap_map();
                    let mut state_string = state.get(TEXT).unwrap().clone().unwrap_string();
                    let mut state_cursor = state.get(CURSOR).unwrap().clone().unwrap_u32();

                    if !state_string.is_empty() {
                        state_cursor += 1;
                    }

                    if state_cursor >= state_string.chars().count() as u32 {
                        state_string.push(c);
                    } else {
                        state_string.insert(
                            state_string
                                .char_indices()
                                .nth(state_cursor as usize)
                                .unwrap()
                                .0,
                            c,
                        )
                    }

                    self.attr(Attribute::Text, AttrValue::String(state_string));
                    self.attr(
                        Attribute::Custom(CURSOR),
                        AttrValue::Payload(PropPayload::One(PropValue::U32(state_cursor))),
                    );

                    CmdResult::Changed(self.state())
                } else {
                    CmdResult::None
                }
            }
            Cmd::Move(direction) | Cmd::Scroll(direction) => {
                let state = self.state().unwrap_map();
                let state_string = state.get(TEXT).unwrap().clone().unwrap_string();
                let mut state_cursor = state.get(CURSOR).unwrap().clone().unwrap_u32();
                let old_cursor = state_cursor;

                match direction {
                    Direction::Down => state_cursor = (state_string.chars().count() - 1) as u32,
                    Direction::Left => {
                        state_cursor = if state_cursor > 0 {
                            state_cursor - 1
                        } else {
                            0
                        }
                    }
                    Direction::Right => {
                        state_cursor = min(
                            (state_string.chars().count().saturating_sub(1)) as u32,
                            state_cursor + 1,
                        )
                    }
                    Direction::Up => state_cursor = 0,
                }

                if old_cursor != state_cursor {
                    self.attr(
                        Attribute::Custom(CURSOR),
                        AttrValue::Payload(PropPayload::One(PropValue::U32(state_cursor))),
                    );

                    CmdResult::Changed(self.state())
                } else {
                    CmdResult::None
                }
            }
            Cmd::GoTo(pos) => {
                let state = self.state().unwrap_map();
                let state_string = state.get(TEXT).unwrap().clone().unwrap_string();

                let state_cursor = match pos {
                    Position::Begin => 0,
                    Position::End => (state_string.chars().count() - 1) as u32,
                    Position::At(at) => {
                        max(0, min((state_string.chars().count() - 1) as u32, at as u32))
                    }
                };

                self.attr(
                    Attribute::Custom(CURSOR),
                    AttrValue::Payload(PropPayload::One(PropValue::U32(state_cursor))),
                );

                CmdResult::Changed(self.state())
            }
            Cmd::Delete => {
                let state = self.state().unwrap_map();
                let mut state_string = state.get(TEXT).unwrap().clone().unwrap_string();
                let mut state_cursor = state.get(CURSOR).unwrap().clone().unwrap_u32();

                if state_string.is_empty() {
                    CmdResult::None
                } else {
                    if state_cursor >= state_string.chars().count() as u32 {
                        state_string.pop();
                    } else {
                        state_string.remove(
                            state_string
                                .char_indices()
                                .nth(state_cursor as usize)
                                .unwrap()
                                .0,
                        );
                    }

                    state_cursor = state_cursor.saturating_sub(1);

                    self.attr(Attribute::Text, AttrValue::String(state_string));
                    self.attr(
                        Attribute::Custom(CURSOR),
                        AttrValue::Payload(PropPayload::One(PropValue::U32(state_cursor))),
                    );

                    CmdResult::Changed(self.state())
                }
            }
            _ => CmdResult::None,
        }
    }
}

impl Component<Message, NoUserEvent> for TextEntry {
    fn on(&mut self, event: Event<NoUserEvent>) -> Option<Message> {
        match event {
            Event::Keyboard(key_event) => match key_event.code {
                Key::Backspace => {
                    if matches!(self.perform(Cmd::Delete), CmdResult::Changed(_)) {
                        Some(Message::Redraw)
                    } else {
                        None
                    }
                }
                Key::Delete => {
                    if matches!(
                        self.perform(Cmd::Move(Direction::Right)),
                        CmdResult::Changed(_)
                    ) {
                        self.perform(Cmd::Delete);
                        Some(Message::Redraw)
                    } else {
                        None
                    }
                }
                Key::Left => {
                    if matches!(
                        self.perform(Cmd::Move(Direction::Left)),
                        CmdResult::Changed(_)
                    ) {
                        Some(Message::Redraw)
                    } else {
                        None
                    }
                }

                Key::Right => {
                    if matches!(
                        self.perform(Cmd::Move(Direction::Right)),
                        CmdResult::Changed(_)
                    ) {
                        Some(Message::Redraw)
                    } else {
                        None
                    }
                }
                Key::Up => {
                    if matches!(
                        self.perform(Cmd::Move(Direction::Up)),
                        CmdResult::Changed(_)
                    ) {
                        Some(Message::Redraw)
                    } else {
                        None
                    }
                }
                Key::Down => {
                    if matches!(
                        self.perform(Cmd::Move(Direction::Down)),
                        CmdResult::Changed(_)
                    ) {
                        Some(Message::Redraw)
                    } else {
                        None
                    }
                }
                Key::Home => {
                    if matches!(
                        self.perform(Cmd::GoTo(Position::Begin)),
                        CmdResult::Changed(_)
                    ) {
                        Some(Message::Redraw)
                    } else {
                        None
                    }
                }
                Key::End => {
                    if matches!(
                        self.perform(Cmd::GoTo(Position::End)),
                        CmdResult::Changed(_)
                    ) {
                        Some(Message::Redraw)
                    } else {
                        None
                    }
                }
                Key::PageUp => {
                    if matches!(
                        self.perform(Cmd::GoTo(Position::Begin)),
                        CmdResult::Changed(_)
                    ) {
                        Some(Message::Redraw)
                    } else {
                        None
                    }
                }
                Key::PageDown => {
                    if matches!(
                        self.perform(Cmd::GoTo(Position::End)),
                        CmdResult::Changed(_)
                    ) {
                        Some(Message::Redraw)
                    } else {
                        None
                    }
                }

                Key::Char(c) => {
                    self.perform(Cmd::Type(
                        if key_event.modifiers.contains(KeyModifiers::SHIFT) {
                            c.to_uppercase().next().unwrap()
                        } else {
                            c
                        },
                    ));

                    Some(Message::Redraw)
                }
                _ => None,
            },
            Event::WindowResize(_, _) => None,
            Event::FocusGained => None,
            Event::FocusLost => None,
            Event::Paste(_) => None,
            _ => None,
        }
    }
}

impl Default for TextEntry {
    fn default() -> Self {
        TextEntry::new(Box::new(|_| true), Box::new(|_| true))
    }
}

impl TextEntry {
    pub fn new(
        char_validator: Box<dyn Fn(char) -> bool>,
        value_validator: Box<dyn Fn(&str) -> bool>,
    ) -> Self {
        Self::with_text(String::new(), char_validator, value_validator)
    }

    pub fn with_text(
        text: String,
        char_validator: Box<dyn Fn(char) -> bool>,
        value_validator: Box<dyn Fn(&str) -> bool>,
    ) -> Self {
        let mut props = Props::default();
        props.set(Attribute::Text, AttrValue::String(text));

        Self {
            props,
            char_validator,
            value_validator,
        }
    }
}

fn padded_span(
    text: &str,
    align: Alignment,
    width: usize,
    padding_style: Style,
    content_style: Style,
) -> Spans {
    if text.len() >= width {
        return Spans::from(Span::styled(
            &text[(text.len() - width)..text.len()],
            content_style,
        ));
    }

    let pad_left = match align {
        Alignment::Left => 0,
        Alignment::Center => (width - text.len()) / 2,
        Alignment::Right => width - text.len(),
    };
    let mut pad_right = match align {
        Alignment::Left => width - text.len(),
        Alignment::Center => (width - text.len()) / 2,
        Alignment::Right => 0,
    };
    if pad_left + pad_right + text.len() < width {
        pad_right = width - (pad_left + text.len());
    }

    Spans::from(vec![
        Span::styled(repeat(' ', pad_left), padding_style),
        Span::styled(text, content_style),
        Span::styled(repeat(' ', pad_right), padding_style),
    ])
}

fn repeat(c: char, count: usize) -> String {
    (0..count).map(|_| c).collect()
}
