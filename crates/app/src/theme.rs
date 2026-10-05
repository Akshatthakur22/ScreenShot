use akshat_core::ThemeChoice;
use gpui::{Global, Hsla, WindowAppearance, rgb, rgba};

/// The screenshots are the colour. The surrounding interface stays quiet,
/// with blue reserved for text that matches the search.
#[derive(Clone, Copy)]
pub struct Theme {
    pub panel: Hsla,
    /// Behind a thumbnail while it loads.
    pub tile: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub faint: Hsla,
    pub hairline: Hsla,
    /// The edge on every thumbnail. Pure black or white at 10%, never a
    /// tinted grey, which picks up the panel and reads as dirt on the edge.
    pub image_edge: Hsla,
    /// Laid over a thumbnail during a search so the found words can stay lit.
    pub veil: Hsla,
    pub accent: Hsla,
    pub accent_wash: Hsla,
    pub hover_wash: Hsla,
    pub keycap: Hsla,
    /// The groove of a switch that's off.
    pub track: Hsla,
}

impl Global for Theme {}

impl Theme {
    /// The choice from settings, falling back to the system's appearance.
    /// AKSHAT_THEME=light or dark beats both, for testing.
    pub fn resolve(choice: ThemeChoice, appearance: WindowAppearance) -> Self {
        match std::env::var("AKSHAT_THEME").as_deref() {
            Ok("light") => return Self::light(),
            Ok("dark") => return Self::dark(),
            _ => {}
        }
        match (choice, appearance) {
            (ThemeChoice::Light, _) => Self::light(),
            (ThemeChoice::Dark, _) => Self::dark(),
            (ThemeChoice::System, WindowAppearance::Dark | WindowAppearance::VibrantDark) => {
                Self::dark()
            }
            (ThemeChoice::System, WindowAppearance::Light | WindowAppearance::VibrantLight) => {
                Self::light()
            }
        }
    }

    fn light() -> Self {
        Self {
            panel: rgb(0xf6f6f4).into(),
            tile: rgb(0xe7e6e2).into(),
            text: rgb(0x18181a).into(),
            muted: rgb(0x6d6c68).into(),
            faint: rgb(0xa9a7a1).into(),
            hairline: rgba(0x1818_1a14).into(),
            image_edge: rgba(0x0000_001a).into(),
            veil: rgba(0x1818_1a70).into(),
            accent: rgb(0x2563eb).into(),
            accent_wash: rgba(0x2563_eb2e).into(),
            hover_wash: rgba(0x1818_1a12).into(),
            keycap: rgb(0xebeae6).into(),
            track: rgb(0xdad9d4).into(),
        }
    }

    fn dark() -> Self {
        Self {
            panel: rgb(0x141416).into(),
            tile: rgb(0x222226).into(),
            text: rgb(0xecebe7).into(),
            muted: rgb(0x9b9a95).into(),
            faint: rgb(0x5f5e5a).into(),
            hairline: rgba(0xffff_ff14).into(),
            image_edge: rgba(0xffff_ff1a).into(),
            veil: rgba(0x0000_0080).into(),
            accent: rgb(0x60a5fa).into(),
            accent_wash: rgba(0x60a5_fa33).into(),
            hover_wash: rgba(0xffff_ff14).into(),
            keycap: rgb(0x26262a).into(),
            track: rgb(0x3a3a3f).into(),
        }
    }
}
