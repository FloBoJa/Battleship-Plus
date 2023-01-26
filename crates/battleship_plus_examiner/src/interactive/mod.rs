use std::net::SocketAddr;

use log::error;
use tuirealm::application::PollStrategy;
use tuirealm::Update;

use app::model::Model;

use crate::interactive::views::server_selection::SeverSelectionMessage;

mod app;
mod components;
mod snowflake;
mod styles;
mod views;

// Let's define the messages handled by our app. NOTE: it must derive `PartialEq`
#[derive(Debug, PartialEq)]
pub enum Message {
    AppClose,
    WindowResized,
    ServerSelectionMessage(SeverSelectionMessage),
}

// Let's define the component ids for our application
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum Id {
    Clock,
    DigitCounter,
    LetterCounter,
    Label,
}

pub(crate) async fn interactive_main(addr: Option<SocketAddr>) -> eyre::Result<()> {
    // The Debug widget cannot notify a redraw. So we help a little
    const FORCE_REDRAW_AFTER: usize = 100; // Ticks
    let mut force_redraw_state = FORCE_REDRAW_AFTER;

    // Setup model
    let mut model = Model::new(addr).await;
    // Enter alternate screen
    model.terminal.enter_alternate_screen()?;
    model.terminal.enable_raw_mode()?;
    // Main loop
    while !model.quit {
        // Tick
        match model.app.tick(PollStrategy::Once) {
            Err(e) => {
                error!("Application Tick: {e}")
            }
            Ok(messages) if !messages.is_empty() => {
                // NOTE: redraw if at least one msg has been processed
                model.redraw = true;
                for msg in messages.into_iter() {
                    let mut msg = Some(msg);
                    while msg.is_some() {
                        msg = model.update(msg);
                    }
                }
            }
            _ => {}
        }
        // Redraw
        force_redraw_state -= 1;
        if model.redraw || force_redraw_state == 0 {
            model.view();
            model.redraw = false;
            force_redraw_state = FORCE_REDRAW_AFTER;
        }
    }
    // Terminate terminal
    model.terminal.leave_alternate_screen()?;
    model.terminal.disable_raw_mode()?;
    model.terminal.clear_screen()?;

    Ok(())
}
