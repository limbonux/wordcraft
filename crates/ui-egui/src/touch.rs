//! Fingers on the page (phones and tablets; #1503, #634), the way phone editors behave: a
//! finger drag scrolls the page and coasts on after a flick, a tap puts the caret there (and so
//! brings up the keyboard), a double tap selects the word, a long press selects the word and
//! shows two handles, and dragging a handle moves that end of the selection. A mouse keeps the
//! desktop behaviour ([`crate::canvas`]'s `mouse`): drag-selecting, clicks, right-clicks.
//!
//! The platform reports touches twice: as [`egui::Event::Touch`] and, for the first finger, as
//! an emulated mouse. While a finger is on the page this module owns the input and the canvas
//! ignores the emulated mouse.

use crate::WordApp;
use crate::canvas::page_at;
use egui::{Event, Pos2, Rect, TouchId, TouchPhase, Ui, Vec2, pos2};
use serde_json::json;
use wordcraft_layout::DocLayout;

/// How far (points) a finger may wander before a press becomes a scroll.
const SLOP: f32 = 10.0;
/// How long (seconds) a finger rests before the press selects a word.
const LONG_PRESS: f64 = 0.5;
/// Two taps closer than this in time (seconds) and distance (points) are a double tap.
const DOUBLE_TAP: (f64, f32) = (0.35, 24.0);
/// A handle's knob: radius, and how far beyond it a finger still grabs it (points).
const KNOB: f32 = 9.0;
const GRAB: f32 = 14.0;
/// Flick coasting: the speed (points/s) below which it stops, and how fast it slows (1/s).
const FLING_MIN: f32 = 30.0;
const FLING_DECAY: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum End {
    Start,
    End,
}

#[derive(Clone, Debug, Default)]
enum Gesture {
    #[default]
    None,
    /// A finger is down and hasn't moved far or long enough to mean anything yet.
    Pending {
        id: TouchId,
        start: Pos2,
        since: f64,
    },
    Scrolling {
        id: TouchId,
        last: Pos2,
        last_time: f64,
        velocity: Vec2,
    },
    /// After a long press: moving the finger grows the selection.
    Selecting {
        id: TouchId,
    },
    Handle {
        id: TouchId,
        end: End,
    },
    /// Two or more fingers: pinch zoom and two-finger scroll are egui's; nothing for us.
    Multi,
}

#[derive(Clone, Debug, Default)]
pub struct Touch {
    gesture: Gesture,
    /// The latest input came from a finger, not a mouse: the selection shows its handles.
    pub active: bool,
    /// The scroll speed (points/s) still coasting after a flick.
    fling: Vec2,
    last_tap: Option<(f64, Pos2)>,
    /// Where the handles' knobs were drawn last frame (screen), for grabbing them.
    knobs: [Option<Pos2>; 2],
    /// The page had the keyboard when the latest finger came down, anywhere in the window.
    had_keyboard: bool,
    /// A tap on the page asked for the keyboard.
    tapped: bool,
}

impl Touch {
    /// A finger owns the page this frame: the canvas leaves the emulated mouse alone.
    pub fn owns_pointer(&self) -> bool {
        !matches!(self.gesture, Gesture::None)
    }

    /// Whether the page may take the keyboard now. On a touch screen taking it brings up the
    /// on-screen keyboard, so as in phone editors only a tap on the page does that; a tap on a
    /// button, a menu or the sheet leaves the keyboard as it was. A mouse always may.
    pub fn may_focus(&mut self) -> bool {
        !self.active || std::mem::take(&mut self.tapped) || self.had_keyboard
    }

    /// Whether a click outside the page (the toolbar, the ribbon) may give the page the keyboard back.
    pub fn may_refocus(&self) -> bool {
        !self.active || self.had_keyboard
    }
}

/// Reads this frame's touches on the canvas (`area`, on screen) and returns how far to scroll
/// it. Runs before the canvas lays out its scroll area, against last frame's page rects.
pub fn frame(app: &mut WordApp, ui: &Ui, area: Rect, layout: &DocLayout) -> Vec2 {
    let (events, now, dt, fingers) = ui.input(|i| (i.events.clone(), i.time, i.stable_dt.clamp(0.0, 0.1), i.multi_touch().map(|m| m.num_touches)));
    let mut touched = false;
    let mut moused = false;
    let mut scroll = Vec2::ZERO;
    if fingers.is_some_and(|n| n > 1) {
        app.canvas.touch.gesture = Gesture::Multi;
    }
    for e in &events {
        match *e {
            Event::Touch { id, phase, pos, .. } => {
                touched = true;
                if phase == TouchPhase::Start {
                    app.canvas.touch.had_keyboard = app.canvas.focused;
                    // A finger on a menu, a dialog or a popup over the page is theirs.
                    if ui.ctx().layer_id_at(pos).is_some_and(|l| l.order != egui::Order::Background) {
                        continue;
                    }
                }
                scroll += touch_event(app, layout, area, id, phase, pos, now);
            }
            Event::PointerMoved(_) | Event::PointerButton { .. } => moused = true,
            _ => {}
        }
    }
    // Emulated mouse events come in the same frame as the touch they stand for.
    if touched {
        app.canvas.touch.active = true;
    } else if moused {
        app.canvas.touch.active = false;
    }
    match app.canvas.touch.gesture.clone() {
        Gesture::Pending { id, start, since } if now - since >= LONG_PRESS => {
            app.canvas.touch.gesture = Gesture::Selecting { id };
            if let Some(pos) = hit(app, layout, start) {
                let pj = serde_json::to_value(&pos).unwrap_or_default();
                let _ = app.session.run("caret.set", &json!({"pos": pj}));
                let _ = app.session.run("select.word", &json!({}));
            }
        }
        Gesture::Pending { since, .. } => {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64((since + LONG_PRESS - now).max(0.01)));
        }
        Gesture::None => {
            let t = &mut app.canvas.touch;
            if t.fling.length() > FLING_MIN {
                scroll += t.fling * dt;
                t.fling *= (-FLING_DECAY * dt).exp();
                ui.ctx().request_repaint();
            } else {
                t.fling = Vec2::ZERO;
            }
        }
        _ => {}
    }
    if fingers.is_none() && matches!(app.canvas.touch.gesture, Gesture::Multi) && !touched {
        app.canvas.touch.gesture = Gesture::None;
    }
    if scroll.is_finite() { scroll } else { Vec2::ZERO }
}

fn pending_id(g: &Gesture) -> TouchId {
    match g {
        Gesture::Pending { id, .. } | Gesture::Scrolling { id, .. } | Gesture::Selecting { id } | Gesture::Handle { id, .. } => *id,
        _ => TouchId(0),
    }
}

fn touch_event(app: &mut WordApp, layout: &DocLayout, area: Rect, id: TouchId, phase: TouchPhase, pos: Pos2, now: f64) -> Vec2 {
    let t = &mut app.canvas.touch;
    let mine = !matches!(t.gesture, Gesture::None | Gesture::Multi) && pending_id(&t.gesture) == id;
    match phase {
        TouchPhase::Start => {
            if !matches!(t.gesture, Gesture::None) || !area.contains(pos) {
                return Vec2::ZERO;
            }
            t.fling = Vec2::ZERO;
            let grabbed = t.knobs.iter().position(|k| k.is_some_and(|k| k.distance(pos) <= KNOB + GRAB));
            t.gesture = match grabbed {
                Some(0) => Gesture::Handle { id, end: End::Start },
                Some(_) => Gesture::Handle { id, end: End::End },
                None => Gesture::Pending { id, start: pos, since: now },
            };
            Vec2::ZERO
        }
        TouchPhase::Move if mine => match t.gesture.clone() {
            Gesture::Pending { start, since, .. } if start.distance(pos) > SLOP => {
                t.gesture = Gesture::Scrolling { id, last: pos, last_time: now.max(since), velocity: Vec2::ZERO };
                // The page follows the finger (the canvas subtracts a scroll delta from its offset).
                pos - start
            }
            Gesture::Scrolling { last, last_time, velocity, .. } => {
                let d = pos - last;
                let step = (now - last_time) as f32;
                // A running average, so one jittery event doesn't decide the flick.
                let velocity = if step > 1e-4 { velocity * 0.6 + d / step * 0.4 } else { velocity };
                t.gesture = Gesture::Scrolling { id, last: pos, last_time: now, velocity };
                d
            }
            Gesture::Selecting { .. } => {
                if let Some(p) = hit(app, layout, pos) {
                    extend(app, p);
                }
                Vec2::ZERO
            }
            Gesture::Handle { end, .. } => {
                if let Some(p) = hit(app, layout, pos) {
                    move_end(app, end, p);
                }
                Vec2::ZERO
            }
            _ => Vec2::ZERO,
        },
        TouchPhase::End | TouchPhase::Cancel if mine => {
            let gesture = std::mem::take(&mut t.gesture);
            match gesture {
                Gesture::Pending { .. } if phase == TouchPhase::End => tap(app, layout, pos, now),
                Gesture::Scrolling { velocity, last_time, .. } => {
                    // A finger that stopped before lifting doesn't fling.
                    t.fling = if now - last_time < 0.1 { velocity } else { Vec2::ZERO };
                }
                _ => {}
            }
            Vec2::ZERO
        }
        _ => Vec2::ZERO,
    }
}

/// A tap puts the caret there; a second tap close by selects the word.
fn tap(app: &mut WordApp, layout: &DocLayout, at: Pos2, now: f64) {
    let double = app.canvas.touch.last_tap.is_some_and(|(t, p)| now - t <= DOUBLE_TAP.0 && p.distance(at) <= DOUBLE_TAP.1);
    app.canvas.touch.last_tap = if double { None } else { Some((now, at)) };
    let Some(pos) = hit(app, layout, at) else { return };
    let pj = serde_json::to_value(&pos).unwrap_or_default();
    let _ = app.run("caret.set", json!({"pos": pj}));
    if double {
        let _ = app.run("select.word", json!({}));
    }
    // The canvas takes the keyboard, which brings up the on-screen one.
    app.canvas.touch.tapped = true;
    app.canvas.want_focus = true;
}

fn hit(app: &WordApp, layout: &DocLayout, p: Pos2) -> Option<wordcraft_doc::Pos> {
    let (page, x, y) = page_at(&app.canvas.page_rects, layout, app.canvas.scale, p)?;
    layout.hit(page, x, y, app.session.sel.focus.story)
}

fn extend(app: &mut WordApp, p: wordcraft_doc::Pos) {
    if p != app.session.sel.focus {
        let pj = serde_json::to_value(&p).unwrap_or_default();
        let _ = app.session.run("caret.set", &json!({"pos": pj, "extend": true}));
    }
}

/// Drag one handle: the other end stays put.
fn move_end(app: &mut WordApp, end: End, p: wordcraft_doc::Pos) {
    let (start, stop) = app.session.sel.ordered();
    let fixed = if end == End::Start { stop } else { start };
    let fj = serde_json::to_value(&fixed).unwrap_or_default();
    let pj = serde_json::to_value(&p).unwrap_or_default();
    let _ = app.session.run("caret.set", &json!({"pos": fj}));
    let _ = app.session.run("caret.set", &json!({"pos": pj, "extend": true}));
}

/// Draws the selection's handles while a finger is in use: a stem under each end and a knob
/// below it to drag.
pub fn paint_handles(app: &mut WordApp, painter: &egui::Painter, layout: &DocLayout, rects: &[Rect], scale: f32) {
    app.canvas.touch.knobs = [None, None];
    if !app.canvas.touch.active || app.session.sel.is_collapsed() || app.session.view.read_mode {
        return;
    }
    let t = crate::theme::Tokens::get(painter.ctx());
    let (start, stop) = app.session.sel.ordered();
    for (i, pos) in [start, stop].iter().enumerate() {
        let Some(c) = layout.caret_on(pos, app.session.page_hint) else { continue };
        let Some(pr) = rects.get(c.page) else { continue };
        let x = pr.min.x + c.x * scale;
        let (top, bottom) = (pr.min.y + c.top * scale, pr.min.y + (c.top + c.height) * scale);
        let knob = pos2(x, bottom + KNOB + 2.0);
        painter.line_segment([pos2(x, top), pos2(x, bottom)], egui::Stroke::new(2.0, t.accent));
        painter.circle_filled(knob, KNOB, t.accent);
        if let Some(k) = app.canvas.touch.knobs.get_mut(i) {
            *k = Some(knob);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::WordApp;
    use egui::{Event, Pos2, TouchDeviceId, TouchId, TouchPhase, pos2};

    fn harness() -> egui_kittest::Harness<'static, WordApp> {
        let doc = wordcraft_engine::sample::sample_document();
        let a = WordApp::new(wordcraft_engine::Session::new(doc), crate::Services::default());
        egui_kittest::Harness::builder().with_size(egui::vec2(390.0, 844.0)).build_ui_state(
            |ui, app: &mut WordApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            a,
        )
    }

    /// A finger as the platform reports it: a touch, and the emulated mouse with it.
    fn finger(h: &mut egui_kittest::Harness<'static, WordApp>, phase: TouchPhase, pos: Pos2) {
        let ev = &mut h.input_mut().events;
        ev.push(Event::PointerMoved(pos));
        match phase {
            TouchPhase::Start => {
                ev.push(Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() })
            }
            TouchPhase::End => {
                ev.push(Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() })
            }
            _ => {}
        }
        ev.push(Event::Touch { device_id: TouchDeviceId(1), id: TouchId(7), phase, pos, force: None });
        h.step();
    }

    fn settle(h: &mut egui_kittest::Harness<'static, WordApp>) {
        for _ in 0..4 {
            h.step();
        }
    }

    /// #1503: a finger dragged up the page scrolls it, and selects nothing.
    #[test]
    fn a_finger_drag_scrolls_instead_of_selecting() {
        let mut h = harness();
        settle(&mut h);
        let before = h.state().canvas.scroll_offset.y;
        finger(&mut h, TouchPhase::Start, pos2(200.0, 600.0));
        for y in [560.0, 500.0, 420.0, 340.0] {
            finger(&mut h, TouchPhase::Move, pos2(200.0, y));
        }
        finger(&mut h, TouchPhase::End, pos2(200.0, 340.0));
        settle(&mut h);
        assert!(h.state().canvas.scroll_offset.y > before + 150.0, "the page scrolled with the finger");
        assert!(h.state().session.sel.is_collapsed(), "nothing was selected");
    }

    /// A tap on a toolbar button leaves the on-screen keyboard down; a tap on the page brings it up.
    #[test]
    fn a_button_tap_leaves_the_keyboard_down() {
        let mut h = harness();
        settle(&mut h);
        h.ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        h.state_mut().canvas.want_focus = false;
        settle(&mut h);
        assert!(!h.state().canvas.focused);
        let bold = egui_kittest::kittest::Queryable::get_by_label(&h, "Bold").rect().center();
        finger(&mut h, TouchPhase::Start, bold);
        finger(&mut h, TouchPhase::End, bold);
        settle(&mut h);
        assert!(!h.state().canvas.focused, "the keyboard stayed down");
        finger(&mut h, TouchPhase::Start, pos2(160.0, 330.0));
        finger(&mut h, TouchPhase::End, pos2(160.0, 330.0));
        settle(&mut h);
        assert!(h.state().canvas.focused, "a tap on the page brought it up");
    }

    /// A tap puts the caret where the finger was; a long press selects the word there and shows
    /// the handles.
    #[test]
    fn a_tap_places_the_caret_and_a_long_press_selects_a_word() {
        let mut h = harness();
        settle(&mut h);
        let start = h.state().session.sel.clone();
        let at = pos2(160.0, 330.0);
        finger(&mut h, TouchPhase::Start, at);
        finger(&mut h, TouchPhase::End, at);
        settle(&mut h);
        assert!(h.state().session.sel.is_collapsed());
        assert_ne!(h.state().session.sel, start, "the tap moved the caret");

        finger(&mut h, TouchPhase::Start, pos2(160.0, 420.0));
        settle(&mut h);
        assert!(!h.state().session.sel.is_collapsed(), "the long press selected a word");
        finger(&mut h, TouchPhase::End, pos2(160.0, 420.0));
        settle(&mut h);
        assert!(h.state().canvas.touch.knobs.iter().all(Option::is_some), "the handles show");
    }
}
