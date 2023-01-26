pub mod styles {
    pub mod invalid {
        use once_cell::sync::Lazy;
        use tuirealm::props::{Color, Style};

        pub static TEXT_PADDING: Lazy<Style> = Lazy::new(|| Style::default().bg(Color::Red));
        pub static TEXT: Lazy<Style> =
            Lazy::new(|| Style::default().bg(Color::Red).fg(Color::Gray));
    }

    pub mod invalid_focus {
        use once_cell::sync::Lazy;
        use tuirealm::props::{Color, Style};

        pub static TEXT_PADDING: Lazy<Style> = Lazy::new(|| Style::default().bg(Color::LightRed));
        pub static TEXT: Lazy<Style> =
            Lazy::new(|| Style::default().bg(Color::LightRed).fg(Color::White));
    }

    pub mod focus {
        use once_cell::sync::Lazy;
        use tuirealm::props::{Color, Style};
        use tuirealm::tui::style::Modifier;

        pub static BOX: Lazy<Style> = Lazy::new(|| {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        });
        pub static TEXT_PADDING: Lazy<Style> = Lazy::new(|| Style::default().bg(Color::Blue));
        pub static TEXT: Lazy<Style> =
            Lazy::new(|| Style::default().bg(Color::Blue).fg(Color::LightYellow));
    }

    pub mod not_focus {
        use once_cell::sync::Lazy;
        use tuirealm::props::{Color, Style};

        pub static BOX: Lazy<Style> = Lazy::new(|| Style::default().fg(Color::DarkGray));

        pub static TEXT_PADDING: Lazy<Style> = Lazy::new(|| Style::default().bg(Color::DarkGray));
        pub static TEXT: Lazy<Style> =
            Lazy::new(|| Style::default().bg(Color::DarkGray).fg(Color::White));
    }
}
