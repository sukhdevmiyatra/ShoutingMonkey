//! The command palette (Ctrl+K): type a few letters of any action, view or
//! filter and press Enter.

use egui::{Key, RichText};

use crate::actions::{self, Action};
use crate::app::App;
use crate::theme;

#[derive(Clone, Copy)]
enum Target {
    Action(Action),
    Go(usize, usize),
}

#[derive(Default)]
pub struct Palette {
    open: bool,
    query: String,
    selected: usize,
    focus: bool,
}

impl Palette {
    pub fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.selected = 0;
        self.focus = true;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    #[cfg(test)]
    pub fn close(&mut self) {
        self.open = false;
    }
}

/// Every word of the query appears in the label (case-insensitive).
fn matches(label: &str, query: &str) -> bool {
    let label = label.to_lowercase();
    query.to_lowercase().split_whitespace().all(|w| label.contains(w))
}

const SHOWN: usize = 12;

impl App {
    pub fn show_palette(&mut self, ctx: &egui::Context) {
        if !self.palette.open {
            return;
        }
        let mut entries: Vec<(String, Option<&'static str>, Target)> = vec![];
        for a in Action::all() {
            entries.push((a.palette_label(), a.blocked(self), Target::Action(a)));
        }
        for (label, v, f) in actions::destinations() {
            entries.push((label, None, Target::Go(v, f)));
        }
        let query = self.palette.query.clone();
        let found: Vec<_> = entries.into_iter().filter(|(l, _, _)| matches(l, &query)).take(SHOWN * 4).collect();
        let count = found.len();
        self.palette.selected = self.palette.selected.min(count.saturating_sub(1));

        let (down, up, enter, esc) = ctx.input(|i| {
            (i.key_pressed(Key::ArrowDown), i.key_pressed(Key::ArrowUp), i.key_pressed(Key::Enter), i.key_pressed(Key::Escape))
        });
        if esc {
            self.palette.open = false;
            return;
        }
        if down && count > 0 {
            self.palette.selected = (self.palette.selected + 1) % count;
        }
        if up && count > 0 {
            self.palette.selected = (self.palette.selected + count - 1) % count;
        }

        let mut chosen = None;
        let top = ctx.content_rect().center_top() + egui::vec2(0.0, 90.0);
        egui::Window::new("Command palette")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .fixed_size([560.0, 0.0])
            .pivot(egui::Align2::CENTER_TOP)
            .fixed_pos(top)
            .show(ctx, |ui| {
                let p = theme::palette(ui);
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.palette.query)
                        .hint_text("Type a command or a view, for example \"broken\" or \"export redirects\"")
                        .desired_width(f32::INFINITY)
                        .margin(egui::vec2(8.0, 6.0)),
                );
                if std::mem::take(&mut self.palette.focus) {
                    edit.request_focus();
                }
                if edit.changed() {
                    self.palette.selected = 0;
                }
                ui.add_space(4.0);
                if found.is_empty() {
                    ui.label(RichText::new("Nothing matches.").color(p.muted));
                }
                // Keep the selection in view.
                let start = self.palette.selected.saturating_sub(SHOWN - 1);
                for (i, (label, blocked, target)) in found.iter().enumerate().skip(start).take(SHOWN) {
                    let selected = i == self.palette.selected;
                    let text = match blocked {
                        Some(why) => RichText::new(format!("{label}  (not available: {why})")).color(p.muted),
                        None => RichText::new(label),
                    };
                    // Left-aligned like a menu: the text goes before a stretch.
                    let mut item = egui::Button::selectable(selected, "").left_text(text).frame_when_inactive(false).min_size(egui::vec2(ui.available_width(), 24.0));
                    if selected {
                        item = item.fill(p.selection);
                    }
                    let resp = ui.add(item);
                    if resp.clicked() {
                        chosen = Some(*target);
                    }
                }
                ui.add_space(2.0);
                ui.label(RichText::new("Up and Down to choose, Enter to run, Esc to close").small().color(p.muted));
            });

        if enter && count > 0 {
            chosen = Some(found[self.palette.selected].2);
        }
        if let Some(t) = chosen {
            self.palette.open = false;
            match t {
                Target::Action(a) => self.run(a, ctx),
                Target::Go(v, f) => self.go_to_view(v, f),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn word_matching() {
        assert!(super::matches("Export reports: Redirect chains and loops", "export redirect"));
        assert!(super::matches("Go to Titles: Missing", "titles miss"));
        assert!(!super::matches("Go to Titles: Missing", "headers"));
    }
}
