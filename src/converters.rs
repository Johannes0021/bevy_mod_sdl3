use bevy_window::WindowTheme;

use sdl3::video::SystemTheme as SdlSystemTheme;

pub fn theme_from_sdl(theme: SdlSystemTheme) -> Option<WindowTheme> {
    match theme {
        SdlSystemTheme::Unknown => None,
        SdlSystemTheme::Light => Some(WindowTheme::Light),
        SdlSystemTheme::Dark => Some(WindowTheme::Dark),
    }
}
