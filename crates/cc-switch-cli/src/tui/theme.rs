use cc_switch_core::AppType;
use ratatui::style::{Color, Modifier, Style};

pub(super) struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub muted: Color,
    pub border: Color,
    pub surface: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub warn: Color,
    pub error: Color,
    pub no_color: bool,
}

impl Theme {
    pub fn for_app(app: &AppType) -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        let light = std::env::var("COLORFGBG")
            .ok()
            .and_then(|v| v.rsplit(';').next()?.parse::<u8>().ok())
            .is_some_and(|v| v == 7 || v == 15);
        let color = |r, g, b| {
            if no_color {
                Color::Reset
            } else {
                Color::Rgb(r, g, b)
            }
        };
        Self {
            bg: if light {
                color(248, 249, 252)
            } else {
                color(40, 42, 54)
            },
            fg: if light {
                color(40, 42, 54)
            } else {
                color(248, 248, 242)
            },
            muted: if light {
                color(92, 102, 140)
            } else {
                color(154, 165, 202)
            },
            border: if light {
                color(164, 170, 190)
            } else {
                color(98, 114, 164)
            },
            surface: if light {
                color(222, 225, 236)
            } else {
                color(68, 71, 90)
            },
            accent: match (app == &AppType::Codex, light) {
                (true, false) => color(80, 250, 123),
                (true, true) => color(24, 138, 66),
                (false, false) => color(255, 121, 198),
                (false, true) => color(186, 36, 120),
            },
            on_accent: if light {
                color(255, 255, 255)
            } else {
                color(40, 42, 54)
            },
            warn: if light {
                color(146, 124, 8)
            } else {
                color(241, 250, 140)
            },
            error: if light {
                color(190, 36, 36)
            } else {
                color(255, 85, 85)
            },
            no_color,
        }
    }

    pub fn base(&self) -> Style {
        Style::default().fg(self.fg).bg(self.bg)
    }
    pub fn selected(&self, focused: bool) -> Style {
        let style = if self.no_color {
            Style::default().add_modifier(Modifier::REVERSED)
        } else if focused {
            Style::default().fg(self.on_accent).bg(self.accent)
        } else {
            Style::default().fg(self.fg).bg(self.surface)
        };
        style.add_modifier(Modifier::BOLD)
    }
}
