//! Visualisations > Force-Directed Crawl Diagram: the crawl's link tree
//! (each page joined to the page it was first found from) laid out with a
//! simple force simulation, in its own window.

use std::collections::HashMap;

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};

use crate::query::Diagram;
use crate::theme::{self, Palette};

const ITERATIONS: u32 = 400;
const STEPS_PER_FRAME: u32 = 4;
const SPRING_LENGTH: f32 = 36.0;
const CELL: f32 = 90.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorBy {
    Indexability,
    Depth,
}

pub struct DiagramWindow {
    pub open: bool,
    pub loading: bool,
    data: Option<Diagram>,
    pos: Vec<Vec2>,
    iteration: u32,
    pan: Vec2,
    zoom: f32,
    color_by: ColorBy,
}

impl Default for DiagramWindow {
    fn default() -> Self {
        Self { open: false, loading: false, data: None, pos: vec![], iteration: 0, pan: Vec2::ZERO, zoom: 1.0, color_by: ColorBy::Indexability }
    }
}

impl DiagramWindow {
    /// Forget the data (a different crawl is being shown); stays open.
    pub fn clear(&mut self) {
        *self = Self { open: self.open, ..Default::default() };
    }

    pub fn set_data(&mut self, d: Diagram) {
        self.pos = initial_layout(&d);
        self.data = Some(d);
        self.iteration = 0;
        self.pan = Vec2::ZERO;
        self.zoom = 1.0;
        self.loading = false;
    }

    /// Show the window; returns a URL the user clicked.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<String> {
        if !self.open {
            return None;
        }
        let mut clicked = None;
        let id = egui::ViewportId::from_hash_of("crawl-diagram");
        let builder = egui::ViewportBuilder::default()
            .with_title("Force-Directed Crawl Diagram")
            .with_inner_size([1040.0, 740.0])
            .with_icon(theme::window_icon());
        ctx.show_viewport_immediate(id, builder, |ui, _class| {
            if ui.input(|i| i.viewport().close_requested()) {
                self.open = false;
            }
            let p = theme::palette(ui);
            egui::Panel::top("diagram-bar")
                .frame(egui::Frame::new().fill(p.toolbar).inner_margin(egui::Margin::symmetric(10, 6)))
                .show(ui, |ui| self.toolbar(ui, p));
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(p.surface))
                .show(ui, |ui| clicked = self.canvas(ui, p));
        });
        clicked
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, p: &Palette) {
        ui.horizontal(|ui| {
            ui.label("Colour by");
            ui.selectable_value(&mut self.color_by, ColorBy::Indexability, "Indexability");
            ui.selectable_value(&mut self.color_by, ColorBy::Depth, "Crawl depth");
            ui.separator();
            if ui.button("Re-run layout").clicked()
                && let Some(d) = &self.data {
                    self.pos = initial_layout(d);
                    self.iteration = 0;
                }
            if ui.button("Fit").clicked() {
                self.pan = Vec2::ZERO;
                self.zoom = 1.0;
            }
            ui.separator();
            match &self.data {
                Some(d) => {
                    let mut text = format!("{} pages", crate::app::fmt_count(d.nodes.len() as u64));
                    if d.truncated {
                        text.push_str(&format!(" (first {} by depth)", crate::app::fmt_count(crate::query::DIAGRAM_LIMIT as u64)));
                    }
                    ui.label(RichText::new(text).color(p.muted));
                }
                None => {
                    ui.label(RichText::new(if self.loading { "Building the diagram…" } else { "Open a crawl first." }).color(p.muted));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new("Drag to move, scroll to zoom, click a page to select it").color(p.muted).small());
            });
        });
    }

    fn canvas(&mut self, ui: &mut egui::Ui, p: &Palette) -> Option<String> {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let d = self.data.as_ref()?;
        if d.nodes.is_empty() {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "No HTML pages in this crawl.", egui::FontId::proportional(14.0), p.muted);
            return None;
        }
        if self.iteration < ITERATIONS {
            for _ in 0..STEPS_PER_FRAME {
                step(d, &mut self.pos, 1.0 - self.iteration as f32 / ITERATIONS as f32);
                self.iteration += 1;
            }
            ui.ctx().request_repaint();
        }

        // View transform: fit the layout's bounds, then user pan/zoom.
        let (min, max) = bounds(&self.pos);
        let size = (max - min).max(vec2(1.0, 1.0));
        let fit = ((rect.width() - 60.0) / size.x).min((rect.height() - 60.0) / size.y).clamp(0.05, 4.0);
        if resp.dragged() {
            self.pan += resp.drag_delta();
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            let pinch = ui.input(|i| i.zoom_delta());
            let factor = pinch * (scroll / 400.0).exp();
            if (factor - 1.0).abs() > f32::EPSILON {
                self.zoom = (self.zoom * factor).clamp(0.2, 12.0);
            }
        }
        let scale = fit * self.zoom;
        let centre = ((min.to_vec2() + max.to_vec2()) / 2.0).to_pos2();
        let pan = self.pan;
        let to_screen = |v: Vec2| rect.center() + (v - centre.to_vec2()) * scale + pan;
        let painter = ui.painter_at(rect);

        let edge = Stroke::new(1.0, p.border);
        for (i, n) in d.nodes.iter().enumerate() {
            if let Some(parent) = n.parent {
                painter.line_segment([to_screen(self.pos[i]), to_screen(self.pos[parent])], edge);
            }
        }
        let pointer = resp.hover_pos();
        let mut hovered: Option<(usize, f32)> = None;
        for (i, n) in d.nodes.iter().enumerate() {
            let c = to_screen(self.pos[i]);
            let r = node_radius(n.inlinks, n.depth) * self.zoom.sqrt();
            let color = self.node_color(p, n);
            painter.circle(c, r, color, Stroke::new(1.0, p.surface));
            if let Some(ptr) = pointer {
                let dist = ptr.distance(c);
                if dist <= r + 3.0 && hovered.is_none_or(|(_, best)| dist < best) {
                    hovered = Some((i, dist));
                }
            }
        }
        self.legend(&painter, rect, p);
        let (i, _) = hovered?;
        let n = &d.nodes[i];
        let c = to_screen(self.pos[i]);
        painter.circle_stroke(c, node_radius(n.inlinks, n.depth) * self.zoom.sqrt() + 3.0, Stroke::new(2.0, p.text));
        let status = n.status.map_or("No response".to_string(), |s| s.to_string());
        resp.clone().on_hover_text(format!(
            "{}\nStatus Code: {status}\nCrawl Depth: {}\nInlinks: {}\n{}",
            n.url,
            n.depth,
            n.inlinks,
            if n.indexable { "Indexable" } else { "Non-indexable" }
        ));
        resp.clicked().then(|| n.url.clone())
    }

    fn node_color(&self, p: &Palette, n: &crate::query::DiagramNode) -> Color32 {
        match self.color_by {
            ColorBy::Indexability => match n.status {
                Some(300..=399) => p.redirect,
                _ if n.indexable => p.ok,
                _ => p.client_error,
            },
            ColorBy::Depth => depth_ramp(p)[(n.depth as usize).min(4)],
        }
    }

    fn legend(&self, painter: &egui::Painter, rect: Rect, p: &Palette) {
        let items: Vec<(Color32, String)> = match self.color_by {
            ColorBy::Indexability => vec![(p.ok, "Indexable".into()), (p.client_error, "Non-indexable".into()), (p.redirect, "Redirect".into())],
            ColorBy::Depth => depth_ramp(p)
                .iter()
                .enumerate()
                .map(|(i, c)| (*c, if i == 4 { "Depth 4+".into() } else { format!("Depth {i}") }))
                .collect(),
        };
        let mut y = rect.top() + 14.0;
        let box_rect = Rect::from_min_size(pos2(rect.left() + 10.0, rect.top() + 6.0), vec2(130.0, 18.0 * items.len() as f32 + 10.0));
        painter.rect_filled(box_rect, 4.0, p.surface.gamma_multiply(0.92));
        painter.rect_stroke(box_rect, 4.0, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
        for (color, label) in items {
            painter.circle_filled(pos2(rect.left() + 24.0, y + 4.0), 5.0, color);
            painter.text(pos2(rect.left() + 36.0, y + 4.0), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(12.0), p.text);
            y += 18.0;
        }
    }
}

use egui::RichText;

/// Ordinal blue ramp for depth 0..4+, from the validated sequential palette.
fn depth_ramp(p: &Palette) -> [Color32; 5] {
    let hex = |v: u32| Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
    if p.dark {
        [hex(0x9EC5F4), hex(0x6DA7EC), hex(0x3987E5), hex(0x256ABF), hex(0x184F95)]
    } else {
        [hex(0x104281), hex(0x1C5CAB), hex(0x2A78D6), hex(0x5598E7), hex(0x86B6EF)]
    }
}

fn node_radius(inlinks: u64, depth: u32) -> f32 {
    let base = if depth == 0 { 9.0 } else { 3.5 };
    (base + (inlinks as f32).sqrt() * 0.6).min(16.0)
}

/// Rings by depth, children spread around their parents' angle.
fn initial_layout(d: &Diagram) -> Vec<Vec2> {
    let mut by_depth: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, n) in d.nodes.iter().enumerate() {
        by_depth.entry(n.depth).or_default().push(i);
    }
    let mut pos = vec![Vec2::ZERO; d.nodes.len()];
    for (depth, members) in by_depth {
        let radius = depth as f32 * SPRING_LENGTH * 2.5;
        let count = members.len().max(1) as f32;
        for (k, i) in members.into_iter().enumerate() {
            let angle = std::f32::consts::TAU * (k as f32 + 0.5) / count;
            pos[i] = vec2(angle.cos(), angle.sin()) * radius;
        }
    }
    pos
}

/// One simulation step: neighbour repulsion (grid-bucketed), springs along
/// tree edges, and a weak pull to the centre. `heat` cools from 1 to 0.
fn step(d: &Diagram, pos: &mut [Vec2], heat: f32) {
    let n = pos.len();
    let mut force = vec![Vec2::ZERO; n];
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (i, p) in pos.iter().enumerate() {
        grid.entry(((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)).or_default().push(i);
    }
    for (&(cx, cy), members) in &grid {
        for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(others) = grid.get(&(cx + dx, cy + dy)) else { continue };
                for &i in members {
                    for &j in others {
                        if i >= j {
                            continue;
                        }
                        let delta = pos[i] - pos[j];
                        let dist2 = delta.length_sq().max(1.0);
                        if dist2 > CELL * CELL {
                            continue;
                        }
                        let push = delta / dist2 * (SPRING_LENGTH * SPRING_LENGTH) * 0.6;
                        force[i] += push;
                        force[j] -= push;
                    }
                }
            }
        }
    }
    for (i, node) in d.nodes.iter().enumerate() {
        if let Some(p) = node.parent {
            let delta = pos[p] - pos[i];
            let dist = delta.length().max(0.01);
            let pull = delta / dist * (dist - SPRING_LENGTH) * 0.08;
            force[i] += pull;
            force[p] -= pull * 0.5;
        }
        force[i] -= pos[i] * 0.002;
    }
    let max_move = 2.0 + 18.0 * heat;
    for i in 0..n {
        // Keep the start page anchored at the centre.
        if d.nodes[i].depth == 0 && i == 0 {
            pos[i] = Vec2::ZERO;
            continue;
        }
        let f = force[i];
        let len = f.length();
        pos[i] += if len > max_move { f / len * max_move } else { f };
    }
}

fn bounds(pos: &[Vec2]) -> (Pos2, Pos2) {
    let mut min = pos2(f32::MAX, f32::MAX);
    let mut max = pos2(f32::MIN, f32::MIN);
    for p in pos {
        min = min.min(p.to_pos2());
        max = max.max(p.to_pos2());
    }
    if min.x > max.x { (Pos2::ZERO, Pos2::ZERO) } else { (min, max) }
}
