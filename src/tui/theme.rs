use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
    pub background: Color,
    pub foreground: Color,
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub border: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    pub user_msg: Color,
    pub assistant_msg: Color,
    pub muted: Color,
}

impl Theme {
    #[must_use]
    pub fn border_style(&self) -> Style {
        Style::default().fg(self.border)
    }
    #[must_use]
    pub fn primary_style(&self) -> Style {
        Style::default().fg(self.primary)
    }
    #[must_use]
    pub fn title_style(&self) -> Style {
        Style::default()
            .fg(self.primary)
            .add_modifier(Modifier::BOLD)
    }
}

/// Catppuccin Mocha palette
#[must_use]
pub fn default_theme() -> Theme {
    Theme {
        name: "catppuccin-mocha",
        background: Color::Rgb(30, 30, 46),
        foreground: Color::Rgb(205, 214, 244),
        primary: Color::Rgb(137, 180, 250),
        secondary: Color::Rgb(166, 227, 161),
        accent: Color::Rgb(245, 194, 231),
        border: Color::Rgb(88, 91, 112),
        selection_bg: Color::Rgb(69, 71, 90),
        selection_fg: Color::Rgb(205, 214, 244),
        error: Color::Rgb(243, 139, 168),
        warning: Color::Rgb(249, 226, 175),
        success: Color::Rgb(166, 227, 161),
        user_msg: Color::Rgb(137, 180, 250),
        assistant_msg: Color::Rgb(166, 227, 161),
        muted: Color::Rgb(108, 112, 134),
    }
}

/// Dracula palette
#[must_use]
pub fn dracula_theme() -> Theme {
    Theme {
        name: "dracula",
        background: Color::Rgb(40, 42, 54),
        foreground: Color::Rgb(248, 248, 242),
        primary: Color::Rgb(189, 147, 249),
        secondary: Color::Rgb(80, 250, 123),
        accent: Color::Rgb(255, 121, 198),
        border: Color::Rgb(68, 71, 90),
        selection_bg: Color::Rgb(68, 71, 90),
        selection_fg: Color::Rgb(248, 248, 242),
        error: Color::Rgb(255, 85, 85),
        warning: Color::Rgb(241, 250, 140),
        success: Color::Rgb(80, 250, 123),
        user_msg: Color::Rgb(139, 233, 253),
        assistant_msg: Color::Rgb(80, 250, 123),
        muted: Color::Rgb(98, 114, 164),
    }
}

/// Nord palette
#[must_use]
pub fn nord_theme() -> Theme {
    Theme {
        name: "nord",
        background: Color::Rgb(46, 52, 64),
        foreground: Color::Rgb(236, 239, 244),
        primary: Color::Rgb(136, 192, 208),
        secondary: Color::Rgb(163, 190, 140),
        accent: Color::Rgb(180, 142, 173),
        border: Color::Rgb(67, 76, 94),
        selection_bg: Color::Rgb(67, 76, 94),
        selection_fg: Color::Rgb(236, 239, 244),
        error: Color::Rgb(191, 97, 106),
        warning: Color::Rgb(235, 203, 139),
        success: Color::Rgb(163, 190, 140),
        user_msg: Color::Rgb(136, 192, 208),
        assistant_msg: Color::Rgb(163, 190, 140),
        muted: Color::Rgb(76, 86, 106),
    }
}

/// Gruvbox dark palette
#[must_use]
pub fn gruvbox_theme() -> Theme {
    Theme {
        name: "gruvbox",
        background: Color::Rgb(40, 40, 40),
        foreground: Color::Rgb(235, 219, 178),
        primary: Color::Rgb(131, 165, 152),
        secondary: Color::Rgb(184, 187, 38),
        accent: Color::Rgb(211, 134, 155),
        border: Color::Rgb(80, 73, 69),
        selection_bg: Color::Rgb(80, 73, 69),
        selection_fg: Color::Rgb(235, 219, 178),
        error: Color::Rgb(251, 73, 52),
        warning: Color::Rgb(250, 189, 47),
        success: Color::Rgb(184, 187, 38),
        user_msg: Color::Rgb(131, 165, 152),
        assistant_msg: Color::Rgb(184, 187, 38),
        muted: Color::Rgb(124, 111, 100),
    }
}

/// Tokyo Night palette
#[must_use]
pub fn tokyo_night_theme() -> Theme {
    Theme {
        name: "tokyo-night",
        background: Color::Rgb(26, 27, 38),
        foreground: Color::Rgb(192, 202, 245),
        primary: Color::Rgb(122, 162, 247),
        secondary: Color::Rgb(158, 206, 106),
        accent: Color::Rgb(187, 154, 247),
        border: Color::Rgb(41, 46, 66),
        selection_bg: Color::Rgb(41, 46, 66),
        selection_fg: Color::Rgb(192, 202, 245),
        error: Color::Rgb(247, 118, 142),
        warning: Color::Rgb(224, 175, 104),
        success: Color::Rgb(158, 206, 106),
        user_msg: Color::Rgb(122, 162, 247),
        assistant_msg: Color::Rgb(158, 206, 106),
        muted: Color::Rgb(86, 95, 137),
    }
}

#[must_use]
pub fn all_themes() -> Vec<Theme> {
    vec![
        default_theme(),
        dracula_theme(),
        nord_theme(),
        gruvbox_theme(),
        tokyo_night_theme(),
    ]
}

pub fn theme_by_name(name: &str) -> Theme {
    all_themes()
        .into_iter()
        .find(|t| t.name == name)
        .unwrap_or_else(default_theme)
}
