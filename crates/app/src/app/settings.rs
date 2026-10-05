//! Onboarding (the first time, when there's no config yet) and settings
//! (ctrl comma, any time after). Both are lists of rows you move through with
//! the arrows, where hovering moves the same highlight, the way a launcher
//! works. Every change is saved as it's made, there's no apply button.

use std::path::PathBuf;
use std::time::Duration;

use akshat_core::{Config, ThemeChoice, tidy};
use gpui::{
    Animation, AnimationExt as _, AnyElement, ClickEvent, Context, CursorStyle, ElementId,
    Focusable as _, FontWeight, MouseMoveEvent, PathPromptOptions, SharedString, Window, div,
    ease_out_quint, prelude::*, px,
};

use super::{Akshat, Page, hint, thousands};
use crate::setup::{self, Candidate, Counts, Service};
use crate::theme::Theme;

#[derive(Clone, Copy)]
pub(super) enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Space,
    Remove,
}

pub(super) struct Settings {
    config: Config,
    cursor: usize,
    service: Service,
    /// How many indexed shots live under each folder, in config order.
    shots: Vec<usize>,
    /// Index and thumbnail cache sizes, filled in from a background thread.
    sizes: Option<(u64, u64)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Folder(usize),
    AddFolder,
    Theme,
    Background,
    Threads,
    ClearThumbs,
}

impl Settings {
    fn rows(&self) -> Vec<Row> {
        let mut rows: Vec<Row> = (0..self.config.folders.len()).map(Row::Folder).collect();
        rows.extend([
            Row::AddFolder,
            Row::Theme,
            Row::Background,
            Row::Threads,
            Row::ClearThumbs,
        ]);
        rows
    }
}

pub(super) struct Onboarding {
    step: Step,
    picks: Vec<Pick>,
    cursor: usize,
    background: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Step {
    Folders,
    Background,
}

struct Pick {
    candidate: Candidate,
    count: Option<Counts>,
    on: bool,
}

impl Onboarding {
    /// Folder rows, then "add a folder". On the second step, yes and no.
    fn row_count(&self) -> usize {
        match self.step {
            Step::Folders => self.picks.len() + 1,
            Step::Background => 2,
        }
    }
}

// Counting past this would only slow the answer down, "10,000+" says enough.
const COUNT_CAP: usize = 10_000;
// A folder with this many screenshot-looking files is ticked up front.
const LOOKS_LIKE_SCREENSHOTS: usize = 10;

impl Akshat {
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let config = Config::load_or_default();
        let paths = self.index.paths().unwrap_or_default();
        let shots = config
            .folders
            .iter()
            .map(|f| paths.iter().filter(|p| p.starts_with(f)).count())
            .collect();
        self.page = Page::Settings(Settings {
            config,
            cursor: 0,
            service: setup::service_status(),
            shots,
            sizes: None,
        });
        window.focus(&self.panel_focus, cx);

        cx.spawn(async move |this, cx| {
            let sizes = cx
                .background_executor()
                .spawn(async move {
                    let data = akshat_core::data_dir().unwrap_or_default();
                    let index =
                        setup::folder_size(&data) - setup::folder_size(&data.join("models"));
                    let thumbs = akshat_core::thumb_path(std::path::Path::new("x"))
                        .ok()
                        .and_then(|p| p.parent().map(setup::folder_size))
                        .unwrap_or(0);
                    (index, thumbs)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Page::Settings(s) = &mut this.page {
                    s.sizes = Some(sizes);
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn start_onboarding(&mut self, cx: &mut Context<Self>) {
        let picks: Vec<Pick> = setup::candidates()
            .into_iter()
            .map(|candidate| Pick {
                candidate,
                count: None,
                on: false,
            })
            .collect();
        let folders: Vec<PathBuf> = picks.iter().map(|p| p.candidate.path.clone()).collect();
        self.page = Page::Onboarding(Onboarding {
            step: Step::Folders,
            picks,
            cursor: 0,
            background: true,
        });
        self.count_folders(folders, cx);
    }

    /// Counts images in the background, then ticks the folders worth reading:
    /// wherever a screenshot tool is set to save, plus any folder that's
    /// clearly full of screenshots going by the file names. Size alone isn't
    /// enough, a Desktop can hold a thousand images that aren't screenshots.
    fn count_folders(&mut self, folders: Vec<PathBuf>, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let counted = cx
                .background_executor()
                .spawn(async move {
                    folders
                        .into_iter()
                        .map(|f| (setup::count_images(&f, COUNT_CAP), f))
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Page::Onboarding(o) = &mut this.page else {
                    return;
                };
                let untouched = o.picks.iter().all(|p| p.count.is_none() && !p.on);
                for (count, folder) in counted {
                    if let Some(pick) = o.picks.iter_mut().find(|p| p.candidate.path == folder) {
                        pick.count = Some(count);
                    }
                }
                if untouched {
                    for pick in &mut o.picks {
                        let counts = pick.count.unwrap_or_default();
                        pick.on = (pick.candidate.tool && counts.images > 0)
                            || counts.screenshots >= LOOKS_LIKE_SCREENSHOTS;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn leave_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = Page::Search;
        window.focus(&self.input.focus_handle(cx), cx);
        cx.notify();
    }

    pub(super) fn panel_key(&mut self, key: Key, window: &mut Window, cx: &mut Context<Self>) {
        match &mut self.page {
            Page::Search => {}
            Page::Settings(s) => {
                let rows = s.rows();
                let row = rows.get(s.cursor).copied();
                match key {
                    Key::Up => s.cursor = s.cursor.saturating_sub(1),
                    Key::Down => s.cursor = (s.cursor + 1).min(rows.len() - 1),
                    Key::Left | Key::Right => {
                        let forward = matches!(key, Key::Right);
                        match row {
                            Some(Row::Theme) => self.cycle_theme(forward, window, cx),
                            Some(Row::Threads) => {
                                self.change_threads(if forward { 1 } else { -1 }, cx)
                            }
                            Some(Row::Background) => self.set_background(forward, cx),
                            _ => {}
                        }
                    }
                    Key::Enter | Key::Space => match row {
                        Some(Row::AddFolder) => self.add_folders(window, cx),
                        Some(Row::Theme) => self.cycle_theme(true, window, cx),
                        Some(Row::Background) => {
                            let on = matches!(s.service, Service::Running);
                            self.set_background(!on, cx)
                        }
                        Some(Row::Threads) => self.change_threads(1, cx),
                        Some(Row::ClearThumbs) => self.clear_thumbnails(cx),
                        Some(Row::Folder(_)) | None => {}
                    },
                    Key::Remove => {
                        if let Some(Row::Folder(i)) = row {
                            self.remove_folder(i, cx);
                        }
                    }
                }
            }
            Page::Onboarding(o) => {
                let last = o.row_count() - 1;
                match (key, o.step) {
                    (Key::Up, _) => o.cursor = o.cursor.saturating_sub(1),
                    (Key::Down, _) => o.cursor = (o.cursor + 1).min(last),
                    (Key::Space, Step::Folders) => {
                        if let Some(pick) = o.picks.get_mut(o.cursor) {
                            pick.on = !pick.on;
                        } else {
                            self.add_folders(window, cx);
                        }
                    }
                    (Key::Enter, Step::Folders) if o.cursor == o.picks.len() => {
                        self.add_folders(window, cx)
                    }
                    (Key::Enter, Step::Folders) => {
                        if o.picks.iter().any(|p| p.on) {
                            o.step = Step::Background;
                            o.cursor = if o.background { 0 } else { 1 };
                        } else {
                            self.flash("pick at least one folder", cx);
                        }
                    }
                    (Key::Space | Key::Left | Key::Right, Step::Background) => {
                        o.background = !o.background;
                        o.cursor = if o.background { 0 } else { 1 };
                    }
                    (Key::Enter, Step::Background) => {
                        o.background = o.cursor == 0;
                        self.finish_onboarding(window, cx);
                    }
                    _ => {}
                }
            }
        }
        cx.notify();
    }

    /// Esc: onboarding steps back, settings goes back to searching.
    pub(super) fn panel_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &mut self.page {
            Page::Onboarding(o) if o.step == Step::Background => {
                o.step = Step::Folders;
                o.cursor = 0;
                cx.notify();
            }
            // Nothing to go back to before the first step, so it just hides,
            // and the next summon lands here again.
            Page::Onboarding(_) => window.remove_window(),
            Page::Settings(_) => self.leave_panel(window, cx),
            Page::Search => {}
        }
    }

    fn finish_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Page::Onboarding(o) = &self.page else {
            return;
        };
        let folders = setup::without_nested(
            o.picks
                .iter()
                .filter(|p| p.on)
                .map(|p| p.candidate.path.clone())
                .collect(),
        );
        let background = o.background;
        let config = Config {
            folders,
            ..Config::default()
        };
        if let Err(e) = config.save() {
            self.flash(format!("couldn't save the config: {e}"), cx);
            return;
        }
        self.theme_choice = config.theme;
        if background && setup::service_status() != Service::Running {
            cx.background_executor()
                .spawn(async { setup::start_service() })
                .detach();
        }
        self.leave_panel(window, cx);
        self.flash(
            if background {
                "reading your screenshots, they show up here as they're read"
            } else {
                "saved, run akshat watch to start reading"
            },
            cx,
        );
    }

    /// Writes the config and puts the live copy in settings in step with it.
    fn save(&mut self, config: Config, cx: &mut Context<Self>) {
        if let Err(e) = config.save() {
            self.flash(format!("couldn't save: {e}"), cx);
            return;
        }
        if let Page::Settings(s) = &mut self.page {
            let paths = self.index.paths().unwrap_or_default();
            s.shots = config
                .folders
                .iter()
                .map(|f| paths.iter().filter(|p| p.starts_with(f)).count())
                .collect();
            s.config = config;
        }
        cx.notify();
    }

    fn current_config(&self) -> Config {
        match &self.page {
            Page::Settings(s) => s.config.clone(),
            _ => Config::load_or_default(),
        }
    }

    fn cycle_theme(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut config = self.current_config();
        let order = [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark];
        let at = order.iter().position(|t| *t == config.theme).unwrap_or(0);
        config.theme = order[if forward { (at + 1) % 3 } else { (at + 2) % 3 }];
        self.set_theme(config, window, cx);
    }

    fn set_theme(&mut self, config: Config, window: &mut Window, cx: &mut Context<Self>) {
        self.theme_choice = config.theme;
        cx.set_global(Theme::resolve(config.theme, window.appearance()));
        self.save(config, cx);
    }

    fn change_threads(&mut self, by: isize, cx: &mut Context<Self>) {
        let mut config = self.current_config();
        let max = std::thread::available_parallelism()
            .map_or(8, |n| n.get())
            .min(16);
        config.threads = (config.threads as isize + by).clamp(1, max as isize) as usize;
        self.save(config, cx);
    }

    fn remove_folder(&mut self, i: usize, cx: &mut Context<Self>) {
        let mut config = self.current_config();
        if i < config.folders.len() {
            let gone = config.folders.remove(i);
            self.flash(format!("stopped reading {}", tidy(&gone)), cx);
            if let Page::Settings(s) = &mut self.page {
                s.cursor = s.cursor.min(config.folders.len());
            }
            self.save(config, cx);
        }
    }

    fn clear_thumbnails(&mut self, cx: &mut Context<Self>) {
        if let Some(dir) = akshat_core::thumb_path(std::path::Path::new("x"))
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
        {
            let _ = std::fs::remove_dir_all(&dir);
        }
        // They come back one by one as tiles are drawn, from the originals.
        if let Page::Settings(s) = &mut self.page
            && let Some((index, _)) = s.sizes
        {
            s.sizes = Some((index, 0));
        }
        self.flash("thumbnails cleared, they'll be redrawn as needed", cx);
    }

    fn set_background(&mut self, on: bool, cx: &mut Context<Self>) {
        let Page::Settings(s) = &mut self.page else {
            return;
        };
        if (s.service == Service::Running) == on {
            return;
        }
        cx.spawn(async move |this, cx| {
            let status = cx
                .background_executor()
                .spawn(async move {
                    if on {
                        setup::start_service()
                    } else {
                        setup::stop_service()
                    };
                    setup::service_status()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Page::Settings(s) = &mut this.page {
                    s.service = status;
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Opens the desktop's folder picker. The overlay sits above every
    /// window, the picker included, so it gets out of the way while you pick
    /// and comes back after, right where it was.
    fn add_folders(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: Some("Read this folder".into()),
        });
        let step_aside = self.floating && crate::is_resident(cx);
        if step_aside {
            window.remove_window();
        }
        cx.spawn(async move |this, cx| {
            let result = picked.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(Ok(Some(paths))) => this.folders_picked(paths, cx),
                Ok(Ok(None)) => {}
                _ => this.flash("couldn't open a folder picker", cx),
            });
            if step_aside {
                cx.update(crate::summon);
            }
        })
        .detach();
    }

    fn folders_picked(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let (paths, refused): (Vec<PathBuf>, Vec<PathBuf>) =
            paths.into_iter().partition(|p| !akshat_core::too_broad(p));
        if let Some(broad) = refused.first() {
            self.flash(
                format!(
                    "{} is everything, pick the folder your screenshots are in",
                    tidy(broad)
                ),
                cx,
            );
        }
        if paths.is_empty() {
            return;
        }
        match &mut self.page {
            Page::Onboarding(o) => {
                let mut fresh = Vec::new();
                for path in paths {
                    if let Some(pick) = o.picks.iter_mut().find(|p| p.candidate.path == path) {
                        pick.on = true;
                    } else {
                        let candidate = Candidate {
                            path: path.clone(),
                            why: "added by you".into(),
                            tool: false,
                        };
                        o.picks.push(Pick {
                            candidate,
                            count: None,
                            on: true,
                        });
                        fresh.push(path);
                    }
                }
                self.count_folders(fresh, cx);
            }
            Page::Settings(_) => {
                let mut config = self.current_config();
                config.folders.extend(paths);
                config.folders = setup::without_nested(config.folders);
                self.save(config, cx);
                self.flash("added, the watcher starts reading it now", cx);
            }
            Page::Search => {}
        }
        cx.notify();
    }

    fn set_cursor(&mut self, at: usize, cx: &mut Context<Self>) {
        let cursor = match &mut self.page {
            Page::Settings(s) => &mut s.cursor,
            Page::Onboarding(o) => &mut o.cursor,
            Page::Search => return,
        };
        if *cursor != at {
            *cursor = at;
            cx.notify();
        }
    }

    /// One selectable line. Hovering selects it, clicking selects it and
    /// does the same thing space would.
    fn row(
        &self,
        ix: usize,
        selected: bool,
        theme: Theme,
        cx: &mut Context<Self>,
        key: Key,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(("row", ix))
            .min_h(px(52.))
            .py_2()
            .px(px(14.))
            .flex()
            .items_center()
            .gap_3()
            .rounded(px(10.))
            .when(selected, |r| r.bg(theme.hover_wash))
            .cursor(CursorStyle::PointingHand)
            .on_mouse_move(
                cx.listener(move |this, _: &MouseMoveEvent, _, cx| this.set_cursor(ix, cx)),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.set_cursor(ix, cx);
                this.panel_key(key, window, cx);
            }))
    }

    pub(super) fn render_settings(&mut self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let Page::Settings(s) = &self.page else {
            return div().into_any_element();
        };
        let rows = s.rows();
        let cursor = s.cursor;
        let config = s.config.clone();
        let shots = s.shots.clone();
        let service = s.service;
        let sizes = s.sizes;

        let mut list = div().flex().flex_col().gap_1();
        let section = |list: gpui::Div, title: &'static str| {
            list.child(
                div()
                    .px(px(14.))
                    .pt(px(18.))
                    .pb(px(4.))
                    .text_xs()
                    .text_color(theme.muted)
                    .child(title),
            )
        };

        list = section(list, "folders it reads");
        for (ix, row) in rows.iter().enumerate() {
            let selected = ix == cursor;
            let el = match *row {
                Row::Folder(i) => {
                    let n = shots.get(i).copied().unwrap_or(0);
                    self.row(ix, selected, theme, cx, Key::Space)
                        .child(
                            div()
                                .flex_1()
                                .text_ellipsis()
                                .child(tidy(&config.folders[i])),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.muted)
                                .child(format!("{} shots", thousands(n))),
                        )
                        .child(
                            div()
                                .id(("remove", i))
                                .opacity(if selected { 1.0 } else { 0.0 })
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.remove_folder(i, cx)
                                }))
                                .child(hint("del", "remove", theme)),
                        )
                        .into_any_element()
                }
                Row::AddFolder => self
                    .row(ix, selected, theme, cx, Key::Enter)
                    .child(
                        div()
                            .flex_1()
                            .text_color(theme.muted)
                            .child("add a folder\u{2026}"),
                    )
                    .into_any_element(),
                Row::Theme => {
                    list = section(list, "look");
                    self.row(ix, selected, theme, cx, Key::Right)
                        .child(div().flex_1().child("theme"))
                        .child(segmented(
                            &["system", "light", "dark"],
                            match config.theme {
                                ThemeChoice::System => 0,
                                ThemeChoice::Light => 1,
                                ThemeChoice::Dark => 2,
                            },
                            theme,
                        ))
                        .into_any_element()
                }
                Row::Background => {
                    list = section(list, "reading");
                    let (on, status): (bool, SharedString) = match service {
                        Service::Running => (
                            true,
                            format!("running, {} searchable", thousands(self.searchable)).into(),
                        ),
                        Service::Stopped => (false, "off, new screenshots won't be read".into()),
                    };
                    self.row(ix, selected, theme, cx, Key::Enter)
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .child("read new screenshots in the background")
                                .child(div().text_xs().text_color(theme.muted).child(status)),
                        )
                        .child(switch("background", on, theme))
                        .into_any_element()
                }
                Row::Threads => {
                    self.row(ix, selected, theme, cx, Key::Enter)
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .child("cores per screenshot")
                                .child(div().text_xs().text_color(theme.muted).child(
                                    "more reads each one faster, fewer leaves more for you",
                                )),
                        )
                        .child(stepper(config.threads, theme))
                        .into_any_element()
                }
                Row::ClearThumbs => {
                    list = section(list, "storage");
                    let text: SharedString = match sizes {
                        Some((index, thumbs)) => {
                            format!("text {}, thumbnails {}", mb(index), mb(thumbs)).into()
                        }
                        None => "measuring\u{2026}".into(),
                    };
                    self.row(ix, selected, theme, cx, Key::Enter)
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .child("clear thumbnails")
                                .child(div().text_xs().text_color(theme.muted).child(text)),
                        )
                        .child(hint("enter", "clear", theme))
                        .into_any_element()
                }
            };
            list = list.child(el);
        }

        div()
            .flex_1()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .h(px(super::HEADER))
                    .px(px(super::PAD + 8.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme.hairline)
                    .child(div().text_size(px(21.)).child("settings"))
                    .child(hint("esc", "back", theme)),
            )
            .child(
                div()
                    .id("settings")
                    .flex_1()
                    .overflow_y_scroll()
                    .flex()
                    .justify_center()
                    .child(div().w(px(620.)).max_w_full().px_4().pb_6().child(list)),
            )
            .child(
                div()
                    .flex_none()
                    .h(px(44.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_5()
                    .text_xs()
                    .text_color(theme.faint)
                    .child(hint("\u{2191} \u{2193}", "move", theme))
                    .child(hint("\u{2190} \u{2192}", "change", theme))
                    .child(div().child(format!("Akshat {}", env!("CARGO_PKG_VERSION")))),
            )
            .into_any_element()
    }

    pub(super) fn render_onboarding(&mut self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let Page::Onboarding(o) = &self.page else {
            return div().into_any_element();
        };
        let step = o.step;
        let cursor = o.cursor;

        let (title, body, rows): (&str, &str, Vec<AnyElement>) = match step {
            Step::Folders => {
                let picks: Vec<(PathBuf, String, Option<Counts>, bool)> = o
                    .picks
                    .iter()
                    .map(|p| {
                        (
                            p.candidate.path.clone(),
                            p.candidate.why.clone(),
                            p.count,
                            p.on,
                        )
                    })
                    .collect();
                let add_ix = picks.len();
                let mut rows: Vec<AnyElement> = picks
                    .into_iter()
                    .enumerate()
                    .map(|(ix, (path, why, count, on))| {
                        let count = match count.map(|c| c.images) {
                            None => "counting\u{2026}".to_string(),
                            Some(COUNT_CAP..) => format!("{}+ images", thousands(COUNT_CAP)),
                            Some(1) => "1 image".into(),
                            Some(n) => format!("{} images", thousands(n)),
                        };
                        self.row(ix, ix == cursor, theme, cx, Key::Space)
                            .child(switch(("pick", ix), on, theme))
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .child(tidy(&path))
                                    .child(div().text_xs().text_color(theme.muted).child(why)),
                            )
                            .child(div().text_sm().text_color(theme.muted).child(count))
                            .into_any_element()
                    })
                    .collect();
                rows.push(
                    self.row(add_ix, cursor == add_ix, theme, cx, Key::Enter)
                        .child(div().w(px(30.)))
                        .child(
                            div()
                                .flex_1()
                                .text_color(theme.muted)
                                .child("add another folder\u{2026}"),
                        )
                        .into_any_element(),
                );
                (
                    "where do your screenshots go?",
                    "Akshat reads every image in the folders you pick, subfolders too, and keeps up with new ones.",
                    rows,
                )
            }
            Step::Background => {
                let options = [
                    (
                        "yes, start it now and at every login",
                        "a small background process reads each new screenshot about a second after you take it, at idle priority",
                    ),
                    (
                        "no, i'll run akshat watch myself",
                        "nothing runs in the background",
                    ),
                ];
                let rows =
                    options
                        .iter()
                        .enumerate()
                        .map(|(ix, (label, detail))| {
                            self.row(ix, ix == cursor, theme, cx, Key::Enter)
                                .child(radio(ix == cursor, theme))
                                .child(
                                    div().flex_1().flex().flex_col().child(*label).child(
                                        div().text_xs().text_color(theme.muted).child(*detail),
                                    ),
                                )
                                .into_any_element()
                        })
                        .collect();
                (
                    "keep reading new ones?",
                    "so a screenshot you take now is searchable a second later.",
                    rows,
                )
            }
        };

        let footer = match step {
            Step::Folders => div()
                .flex()
                .gap_5()
                .child(hint("space", "pick", theme))
                .child(hint("enter", "continue", theme)),
            Step::Background => div()
                .flex()
                .gap_5()
                .child(hint("esc", "back", theme))
                .child(hint("enter", "done", theme)),
        };

        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .id("onboarding")
                    .flex_1()
                    .w(px(620.))
                    .max_w_full()
                    .px_4()
                    .pt(px(56.))
                    .overflow_y_scroll()
                    .child(div().px(px(14.)).text_xs().text_color(theme.muted).child(
                        if step == Step::Folders {
                            "1 of 2"
                        } else {
                            "2 of 2"
                        },
                    ))
                    .child(
                        div()
                            .px(px(14.))
                            .pt_2()
                            .text_size(px(26.))
                            .line_height(px(34.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(title),
                    )
                    .child(
                        div()
                            .px(px(14.))
                            .pt_2()
                            .pb_6()
                            .text_color(theme.muted)
                            .child(body),
                    )
                    .child(div().flex().flex_col().gap_1().children(rows)),
            )
            .child(
                div()
                    .flex_none()
                    .h(px(56.))
                    .flex()
                    .items_center()
                    .child(footer),
            )
            .into_any_element()
    }
}

fn mb(bytes: u64) -> String {
    format!("{} MB", (bytes as f64 / 1_048_576.0).round() as u64)
}

/// On is ink, off is a grey groove, and the knob slides between the two.
fn switch(id: impl Into<ElementId>, on: bool, theme: Theme) -> impl IntoElement {
    let (from, to) = if on { (2.0, 14.0) } else { (14.0, 2.0) };
    let id: ElementId = id.into();
    div()
        .flex_none()
        .relative()
        .w(px(30.))
        .h(px(18.))
        .rounded_full()
        .bg(if on { theme.text } else { theme.track })
        .child(
            div()
                .absolute()
                .top(px(2.))
                .size(px(14.))
                .rounded_full()
                .bg(if on { theme.panel } else { theme.muted })
                .with_animation(
                    ElementId::Name(format!("{id:?}-{on}").into()),
                    Animation::new(Duration::from_millis(140)).with_easing(ease_out_quint()),
                    move |knob, t| knob.left(px(from + (to - from) * t)),
                ),
        )
}

fn radio(on: bool, theme: Theme) -> impl IntoElement {
    div()
        .flex_none()
        .size(px(16.))
        .rounded_full()
        .border_1()
        .border_color(if on { theme.text } else { theme.faint })
        .flex()
        .items_center()
        .justify_center()
        .when(on, |r| {
            r.child(div().size(px(8.)).rounded_full().bg(theme.text))
        })
}

fn segmented(options: &[&'static str], selected: usize, theme: Theme) -> impl IntoElement {
    // The pill sits 3 px inside the track, so its radius is the track's minus 3.
    div()
        .flex()
        .p(px(3.))
        .rounded(px(9.))
        .bg(theme.keycap)
        .children(options.iter().enumerate().map(|(i, label)| {
            div()
                .px_3()
                .py(px(3.))
                .rounded(px(6.))
                .text_sm()
                .when(i == selected, |s| s.bg(theme.text).text_color(theme.panel))
                .when(i != selected, |s| s.text_color(theme.muted))
                .child(*label)
        }))
}

fn stepper(value: usize, theme: Theme) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(hint("\u{2190}", "", theme))
        .child(
            div()
                .w(px(20.))
                .flex()
                .justify_center()
                .child(value.to_string()),
        )
        .child(hint("\u{2192}", "", theme))
}
