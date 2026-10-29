//! The compact layout for narrow windows: a phone, or a narrow browser tab (#634, #1574).
//!
//! Below [`COMPACT_WIDTH`] the window is laid out as phone editors are: the title bar keeps only
//! Quick Access, Search and the account; the ribbon gives way to one row of large touch buttons
//! at the bottom, within the thumb's reach, with More at its end, which slides up a sheet of every
//! command: the tabs across its top and the tab's groups stacked under them; there is no status
//! bar or rulers; and the document reflows to the window in Web Layout, since a printed page
//! doesn't fit a phone. Widening the window brings the desktop layout and the earlier view back.

use crate::WordApp;
use crate::icons;
use crate::theme::{Tokens, medium, semibold};
use crate::widgets;
use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use wordcraft_layout::ViewMode;

/// Windows narrower than this (in points) get the compact layout.
pub const COMPACT_WIDTH: f32 = 600.0;
/// A touch target: the compact toolbar's buttons are at least this wide and tall.
const BUTTON: f32 = 40.0;

pub fn is_compact(ctx: &egui::Context) -> bool {
    ctx.content_rect().width() < COMPACT_WIDTH
}

/// Becoming compact switches Print Layout to Web Layout; widening the window again gives the
/// earlier view back, unless the user picked another view in between.
pub fn follow_width(app: &mut WordApp, ctx: &egui::Context) {
    let compact = is_compact(ctx);
    let mode = app.session.view.mode;
    match (compact, app.compact_prev_mode) {
        (true, None) => {
            app.compact_prev_mode = Some(mode);
            if mode == ViewMode::Print && !app.session.view.read_mode {
                let _ = app.run("view.webLayout", json!({}));
            }
        }
        (false, Some(prev)) => {
            app.compact_prev_mode = None;
            app.compact_ribbon = false;
            if prev == ViewMode::Print && mode == ViewMode::Web {
                let _ = app.run("view.printLayout", json!({}));
            }
        }
        _ => {}
    }
}

/// The compact toolbar: the formatting used most, as large buttons in one row that scrolls
/// sideways, and More at its end, which shows or hides the full ribbon under it.
pub fn toolbar(app: &mut WordApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::bottom("compact_toolbar")
        .exact_size(BUTTON + 8.0)
        .frame(egui::Frame::NONE.fill(t.ribbon).inner_margin(egui::Margin::symmetric(4, 4)).stroke(Stroke::new(1.0, t.border)))
        .show(ui, |ui| {
            // No document open (#1129): nothing to format.
            if !app.has_document() {
                ui.disable();
            }
            // More keeps its place at the end; the buttons before it scroll.
            let full = ui.max_rect();
            let more_rect = Rect::from_min_max(egui::pos2(full.max.x - BUTTON, full.min.y), full.max);
            let row = Rect::from_min_max(full.min, egui::pos2(more_rect.min.x - 4.0, full.max.y));
            ui.scope_builder(egui::UiBuilder::new().max_rect(more_rect), |ui| {
                if button(ui, app, "chevronUp", "More commands", "ui.compactRibbon", false) {
                    app.compact_ribbon = true;
                }
            });
            ui.scope_builder(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Min)), |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("compact_toolbar_row")
                    .max_height(BUTTON)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                            buttons(app, ui);
                        });
                    });
            });
        });
}

/// The tabs the sheet offers: File (the Backstage), the user's tabs, then the contextual ones.
fn sheet_tabs(app: &WordApp) -> Vec<(String, bool)> {
    let mut tabs: Vec<(String, bool)> = vec![("File".to_string(), false)];
    tabs.extend(app.session.ribbon.tabs().iter().filter(|e| !e.hidden).map(|e| (e.name.clone(), e.custom)));
    for ct in crate::ribbon::contextual_tabs(&app.session) {
        if !tabs.iter().any(|(n, _)| n == ct) {
            tabs.push((ct.to_string(), false));
        }
    }
    tabs
}

/// More: a sheet over the lower part of the window with every command. The tabs run across its
/// top (they scroll sideways), with the button that closes it; the tab's groups stack under them,
/// each with its title, and scroll.
pub fn sheet(app: &mut WordApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let height = (ui.ctx().content_rect().height() * 0.5).clamp(240.0, 460.0);
    egui::Panel::bottom("compact_sheet").exact_size(height).frame(egui::Frame::NONE.fill(t.ribbon).stroke(Stroke::new(1.0, t.border))).show(
        ui,
        |ui| {
            if !app.has_document() && app.ui.tab != "Help" {
                ui.disable();
            }
            let full = ui.max_rect();
            let header = Rect::from_min_max(full.min, pos2(full.max.x, full.min.y + BUTTON + 4.0));
            let close = Rect::from_min_max(pos2(header.max.x - BUTTON - 4.0, header.min.y + 2.0), pos2(header.max.x - 4.0, header.max.y - 2.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(close), |ui| {
                if button(ui, app, "chevronDown", "Hide the ribbon", "ui.compactRibbon", false) {
                    app.compact_ribbon = false;
                    app.canvas.want_focus = true;
                }
            });
            let strip = Rect::from_min_max(pos2(header.min.x + 4.0, header.min.y), pos2(close.min.x - 4.0, header.max.y));
            ui.scope_builder(egui::UiBuilder::new().max_rect(strip).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("compact_sheet_tabs")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.horizontal_centered(|ui| {
                            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                            for (tab, custom) in sheet_tabs(app) {
                                tab_chip(app, ui, &t, &tab, custom);
                            }
                        });
                    });
            });
            ui.painter().hline(full.x_range(), header.max.y, Stroke::new(1.0, t.border));
            let body = Rect::from_min_max(pos2(full.min.x + 12.0, header.max.y + 1.0), full.max);
            ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
                egui::ScrollArea::both().id_salt("compact_sheet_groups").auto_shrink([false, false]).show(ui, |ui| {
                    ui.ctx().data_mut(|d| d.insert_temp(sheet_key(), true));
                    ui.vertical(|ui| crate::ribbon::tab_body(app, ui));
                    ui.ctx().data_mut(|d| d.insert_temp(sheet_key(), false));
                });
            });
        },
    );
}

fn tab_chip(app: &mut WordApp, ui: &mut Ui, t: &Tokens, tab: &str, custom: bool) {
    // A custom tab's name is the user's own text: shown as typed.
    let shown = if custom { tab } else { tl!(tab) };
    let active = app.ui.tab == tab;
    let font = if active { semibold(14.0) } else { medium(14.0) };
    let w = ui.ctx().fonts_mut(|f| f.layout_no_wrap(shown.to_string(), font.clone(), t.text).size().x) + 24.0;
    let (r, _) = ui.allocate_exact_size(vec2(w, BUTTON), Sense::hover());
    let resp = ui.interact(r, egui::Id::new(("compact_sheet_tab", tab)), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, active, shown));
    if active {
        ui.painter().rect_filled(Rect::from_min_max(pos2(r.min.x + 8.0, r.max.y - 3.0), pos2(r.max.x - 8.0, r.max.y)), 1.5, t.accent);
    } else if resp.hovered() {
        ui.painter().rect_filled(r.shrink2(vec2(0.0, 4.0)), 4.0, t.hover);
    }
    ui.painter().text(r.center(), Align2::CENTER_CENTER, shown, font, if active { t.accent_text } else { t.text });
    if resp.clicked() {
        if tab == "File" {
            let _ = app.run("ui.backstage", json!({"value": true}));
        } else {
            let _ = app.run("ui.tab", json!({"tab": tab}));
        }
    }
}

fn sheet_key() -> egui::Id {
    egui::Id::new("compact_sheet_groups")
}

/// The ribbon's groups are being drawn into the sheet ([`sheet_group`] instead of a column).
pub fn in_sheet(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(sheet_key())).unwrap_or(false)
}

/// A ribbon group in the sheet: its title over its commands, a rule under them. The commands
/// keep the ribbon's controls and menus; a group wider than the window scrolls with the sheet.
pub fn sheet_group(ui: &mut Ui, title: &str, launcher: Option<&str>, app: &mut WordApp, add: impl FnOnce(&mut Ui, &mut WordApp)) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).font(semibold(13.0)).color(t.group_label));
        if let Some(cmd) = launcher {
            let (r, resp) = ui.allocate_exact_size(vec2(28.0, 22.0), Sense::click());
            icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(13.0, 13.0)), "launcher", t.text_dim, t.accent);
            let tip = crate::i18n::fmt(tl!("{group} settings"), &[("group", title)]);
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &tip));
            if resp.on_hover_text(tip).clicked() {
                let _ = app.run(cmd, json!({}));
            }
        }
    });
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
        ui.set_min_height(widgets::CONTENT_H);
        add(ui, app);
    });
    ui.add_space(6.0);
    let y = ui.cursor().min.y;
    ui.painter().hline(ui.max_rect().x_range(), y, Stroke::new(1.0, t.border));
}

fn buttons(app: &mut WordApp, ui: &mut Ui) {
    use wordcraft_doc::Align as A;
    let st = app.session.run("format.state", &json!({})).unwrap_or_default();
    let flag = |k: &str| st.get(k).and_then(Value::as_bool).unwrap_or(false);
    let rp = app.session.doc.para_at(&app.session.sel.focus).map(|p| app.session.doc.styles.resolve_para(&p.props));
    // Alignment as seen on the page, like the ribbon (Align Right is a right-to-left paragraph's start).
    let align = rp.as_ref().map(|r| r.align.visual(r.bidi)).unwrap_or_default();
    let rtl = rp.as_ref().is_some_and(|r| r.bidi);
    let lists = wordcraft_engine::cmd::para::list_buttons(&app.session.doc, rp.as_ref().and_then(|r| r.numbering));
    let groups: [&[(&str, &str, &str, bool)]; 5] = [
        &[
            ("bold", "Bold", "format.bold", flag("bold")),
            ("italic", "Italic", "format.italic", flag("italic")),
            ("underline", "Underline", "format.underline", flag("underline")),
        ],
        &[("grow", "Increase Font Size", "format.growFont", false), ("shrink", "Decrease Font Size", "format.shrinkFont", false)],
        &[
            ("alignLeft", "Align Left", "para.alignLeft", align == A::Left),
            ("alignCenter", "Center", "para.alignCenter", align == A::Center),
            ("alignRight", "Align Right", "para.alignRight", align == A::Right),
            ("justify", "Justify", "para.justify", align == A::Justify),
        ],
        &[("bullets", "Bullets", "para.bullets", lists.bullets), ("numbering", "Numbering", "para.numbering", lists.numbering)],
        &[("textLtr", "Left-to-Right Text Direction", "para.ltr", !rtl), ("textRtl", "Right-to-Left Text Direction", "para.rtl", rtl)],
    ];
    for (i, group) in groups.iter().enumerate() {
        if i > 0 {
            separator(ui);
        }
        for &(icon, tip, id, checked) in *group {
            if button(ui, app, icon, tip, id, checked) && widgets::enabled(app, id) {
                widgets::run_from_ribbon(app, id, json!({}));
            }
        }
    }
}

/// A large icon button; true when clicked. `id` greys it out when its command can't run.
fn button(ui: &mut Ui, app: &WordApp, icon: &str, tip: &str, id: &str, checked: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let tip = tl!(tip);
    let (r, resp) = ui.allocate_exact_size(vec2(BUTTON, BUTTON), Sense::click());
    let on = widgets::enabled(app, id);
    widgets::bg(ui, r, &resp, checked, &t);
    let (c, a) = if on { (t.icon, t.accent) } else { (t.text_disabled, t.text_disabled) };
    icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(22.0, 22.0)), icon, c, a);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, on, checked, tip));
    resp.on_hover_text(tip).clicked()
}

fn separator(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(9.0, BUTTON), Sense::hover());
    ui.painter().vline(r.center().x, r.y_range().shrink(8.0), Stroke::new(1.0, t.border));
}

#[cfg(test)]
mod tests {
    use crate::WordApp;
    use egui_kittest::kittest::Queryable;
    use serde_json::json;
    use wordcraft_layout::ViewMode;

    fn harness(a: WordApp, w: f32) -> egui_kittest::Harness<'static, WordApp> {
        egui_kittest::Harness::builder().with_size(egui::vec2(w, 844.0)).build_ui_state(
            |ui, app: &mut WordApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            a,
        )
    }

    fn app() -> WordApp {
        let doc = wordcraft_engine::sample::sample_document();
        WordApp::new(wordcraft_engine::Session::new(doc), crate::Services::default())
    }

    /// #634: a phone-sized window gets the compact toolbar instead of the ribbon, in Web Layout
    /// wrapped to the window; More shows the ribbon; a wide window gets the desktop layout and
    /// Print Layout back.
    #[test]
    fn a_narrow_window_gets_the_compact_layout_and_a_wide_one_gives_it_back() {
        let mut h = harness(app(), 390.0);
        for _ in 0..4 {
            h.step();
        }
        assert_eq!(h.state().session.view.mode, ViewMode::Web, "a phone shows the text, not the printed page");
        assert!(h.state().session.view.web_width < 390.0, "the text wraps to the window");
        assert!(h.query_by_label("Bold").is_some(), "the compact toolbar is up");
        assert!(h.query_by_label("Clipboard").is_none(), "the ribbon is hidden");
        h.get_by_label("More commands").click();
        for _ in 0..3 {
            h.step();
        }
        assert!(h.state().compact_ribbon, "More opened the sheet");
        assert!(h.query_by_label("Clipboard").is_some(), "the sheet lists the tab's groups");
        assert!(h.query_by_label("Hide the ribbon").is_some());

        h.set_size(egui::vec2(1440.0, 900.0));
        for _ in 0..4 {
            h.step();
        }
        assert_eq!(h.state().session.view.mode, ViewMode::Print, "widening gives the page back");
        assert!(h.query_by_label("More commands").is_none(), "the compact toolbar is gone");
        assert!(!h.state().compact_ribbon);
    }

    /// In a phone-sized window File lists its pages across the top: they open, and Back leaves.
    #[test]
    fn a_narrow_window_lists_the_file_pages_across_the_top() {
        let mut h = harness(app(), 390.0);
        h.state_mut().run("ui.backstage", json!({"value": true, "page": "home"})).unwrap();
        for _ in 0..4 {
            h.step();
        }
        h.get_by_label("Info").click();
        for _ in 0..3 {
            h.step();
        }
        assert_eq!(h.state().ui.backstage_page, "info");
        // Back is the first square of the bar; pressed with the pointer, as a finger would.
        let back = h.get_by_label("Back").rect().center();
        for pressed in [true, false] {
            h.input_mut().events.push(egui::Event::PointerMoved(back));
            h.input_mut().events.push(egui::Event::PointerButton {
                pos: back,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
            h.step();
        }
        for _ in 0..3 {
            h.step();
        }
        assert!(!h.state().ui.backstage, "Back left File");
    }

    /// A view picked while compact is kept when the window widens.
    #[test]
    fn a_view_picked_while_compact_is_kept() {
        let mut h = harness(app(), 390.0);
        for _ in 0..4 {
            h.step();
        }
        h.state_mut().run("view.readMode", json!({})).unwrap();
        h.step();
        h.set_size(egui::vec2(1440.0, 900.0));
        for _ in 0..4 {
            h.step();
        }
        assert!(h.state().session.view.read_mode);
    }
}
