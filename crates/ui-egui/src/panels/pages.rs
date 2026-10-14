//! Pages panel: parents at the top, document spreads with live thumbnails below.

use std::collections::HashMap;
use std::sync::Mutex;

use egui::{Color32, Sense, Stroke, StrokeKind, vec2};
use serde_json::json;

use crate::theme::Tokens;
use crate::{DesignApp, icons};

type ThumbKey = (u64, u64, usize);

static THUMBS: Mutex<Option<HashMap<ThumbKey, egui::TextureHandle>>> = Mutex::new(None);

fn thumb(app: &mut DesignApp, ctx: &egui::Context, abs: usize, h: f32) -> Option<egui::TextureHandle> {
    let st = app.session.active()?;
    let key = (st.uid, st.revision, abs);
    let mut g = THUMBS.lock().unwrap_or_else(|e| e.into_inner());
    let map = g.get_or_insert_with(HashMap::new);
    if let Some(t) = map.get(&key) {
        return Some(t.clone());
    }
    let page = st.doc.page(abs)?;
    let scale = (h as f64 * ctx.pixels_per_point() as f64) / page.height;
    let mut r = designcraft_render::Renderer::new();
    r.threads = 0;
    let img = r.render_page(
        &st.doc,
        &app.session.cache,
        abs,
        scale,
        false,
        &designcraft_render::RenderOptions { printing_only: true, ..Default::default() },
    )?;
    let ci = egui::ColorImage::from_rgba_premultiplied([img.width as usize, img.height as usize], &img.pixels);
    let tex = ctx.load_texture(format!("thumb{abs}"), ci, egui::TextureOptions::LINEAR);
    // Drop thumbnails of older revisions.
    let (uid, rev) = (st.uid, st.revision);
    map.retain(|k, _| !(k.0 == uid && k.1 != rev));
    map.insert(key, tex.clone());
    Some(tex)
}

/// What is being dragged in the Pages panel.
#[derive(Clone, Debug, PartialEq)]
enum PageDrag {
    /// A document page (absolute index).
    Page(usize),
    /// A parent (prefix; `None` = [None]).
    Parent(Option<String>),
}

/// A spread's label under its thumbnails, as InDesign's: one name for a single page, "2–3" for
/// pages numbered on in one section (an en dash), and the names apart ("2, 1", "8, A") when a section starts
/// inside the spread.
pub(crate) fn spread_label(app: &DesignApp, doc: &designcraft_doc::Document, first: usize, n: usize) -> String {
    let names: Vec<String> = (first..first + n).map(|abs| app.session.page_label(abs)).collect();
    let starts = |abs: usize| abs == 0 || doc.sections.iter().any(|s| s.start == abs);
    match (names.first(), names.last()) {
        (Some(a), Some(b)) if names.len() > 1 => {
            if !app.session.prefs.absolute_page_numbers && (first + 1..first + n).any(starts) {
                names.join(", ")
            } else {
                format!("{a}\u{2013}{b}")
            }
        }
        (Some(a), _) => a.clone(),
        _ => String::new(),
    }
}

/// Selected-page tint (multiplied over the thumbnail).
const SELECTED_TINT: Color32 = Color32::from_rgb(0x88, 0xc7, 0xfb);
/// Page-number badge of the selected page.
const BADGE: Color32 = Color32::from_rgb(0x2f, 0x73, 0xe0);
/// Thumbnail height (InDesign: about 28×37 pt for Letter).
const THUMB_H: f32 = 37.0;
/// Parent rows.
const ROW_H: f32 = 29.0;

/// Paint a blank page icon of `n` pages (parents list), right-aligned at `right`.
fn blank_spread(p: &egui::Painter, right: egui::Pos2, n: usize, aspect: f32, t: &Tokens) {
    let h = 24.0;
    let w = h * aspect;
    let n = n.max(1);
    for k in 0..n {
        let r = egui::Rect::from_min_size(egui::pos2(right.x - w * (n - k) as f32, right.y - h / 2.0), vec2(w, h));
        p.rect_filled(r, 0.0, Color32::WHITE);
        p.rect_stroke(r, 0.0, Stroke::new(0.5, t.border), StrokeKind::Inside);
    }
}

pub fn show(app: &mut DesignApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(st) = app.session.active() else { return };
    let doc = st.doc.clone();
    let editing_parents = st.editing_parents;
    let aspect = (doc.settings.page_width / doc.settings.page_height.max(1.0)) as f32;
    let mut drop: Option<(PageDrag, usize)> = None;
    ui.spacing_mut().item_spacing.y = 0.0;
    // ---- Parents: [None] and each parent, 29 pt rows with a blank thumbnail at the right.
    let mut rows: Vec<(String, Option<String>, usize)> = vec![("[None]".into(), None, 1)];
    for p in &doc.parents {
        let label = p.parent.as_ref().map(|i| i.label()).unwrap_or_default();
        rows.push((label, p.parent.as_ref().map(|i| i.prefix.clone()), p.pages.len()));
    }
    for (label, prefix, n) in rows {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click_and_drag());
        let active = editing_parents && prefix.is_some();
        if active {
            ui.painter().rect_filled(r, 0.0, t.row_selected);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 0.0, t.hover);
        }
        ui.painter().text(r.left_center() + vec2(3.0, 0.0), egui::Align2::LEFT_CENTER, &label, egui::FontId::proportional(11.5), t.text_strong);
        blank_spread(ui.painter(), egui::pos2(r.max.x - 10.0, r.center().y), n, aspect, &t);
        resp.dnd_set_drag_payload(PageDrag::Parent(prefix.clone()));
        if resp.double_clicked() && prefix.is_some() {
            let _ = app.run("layout.parents.edit", json!({"on": true}));
            app.views.clear();
        }
        if resp.dragged() {
            drag_ghost(ui, &label, &t);
        }
    }
    ui.add_space(4.0);
    let r = ui.available_rect_before_wrap();
    ui.painter().rect_filled(egui::Rect::from_min_size(egui::pos2(r.min.x - 10.0, r.min.y), vec2(r.width() + 20.0, 1.5)), 0.0, t.section_divider);
    ui.add_space(12.0);
    // ---- Document spreads.
    let cur = crate::canvas::current_page(app).unwrap_or(0);
    let cur_spread = doc.page_loc(cur).map(|l| l.0);
    let followed_id = egui::Id::new("pages_panel_followed");
    let center = ui.available_width() / 2.0;
    let dragging = egui::DragAndDrop::has_any_payload(ui.ctx());
    for (si, sp) in doc.spreads.iter().enumerate() {
        let first = doc.first_page_of_spread(si);
        let scale = THUMB_H / doc.settings.page_height.max(1.0) as f32;
        let spine = sp.spine_x() as f32;
        let (row, row_resp) = ui.allocate_exact_size(vec2(ui.available_width(), 8.0 + THUMB_H + 22.0), Sense::hover());
        let top = row.min.y + 8.0;
        // The panel follows the page in view (InDesign scrolls to the selected page).
        if cur_spread == Some(si) && !editing_parents && ui.data(|d| d.get_temp::<usize>(followed_id)) != Some(cur) {
            row_resp.scroll_to_me(None);
            ui.data_mut(|d| d.insert_temp(followed_id, cur));
        }
        for (pi, p) in sp.pages.iter().enumerate() {
            let abs = first + pi;
            let x0 = row.min.x + center + (p.x as f32 - spine) * scale;
            let pr = egui::Rect::from_min_size(egui::pos2(x0, top), vec2(p.width as f32 * scale, p.height as f32 * scale));
            let resp = ui.interact(pr, ui.id().with(("page", abs)), Sense::click_and_drag());
            let selected = abs == cur && !editing_parents;
            let tint = if selected { SELECTED_TINT } else { Color32::WHITE };
            if let Some(tex) = thumb(app, ui.ctx(), abs, THUMB_H) {
                ui.painter().image(tex.id(), pr, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), tint);
            } else {
                ui.painter().rect_filled(pr, 0.0, tint);
            }
            ui.painter().rect_stroke(pr, 0.0, Stroke::new(0.5, Color32::from_gray(0x4a)), StrokeKind::Outside);
            // A section starts here: InDesign's small triangle above the page.
            if abs == 0 || doc.sections.iter().any(|s| s.start == abs) {
                let c = egui::pos2(pr.center().x, row.min.y + 4.0);
                ui.painter().add(egui::Shape::convex_polygon(
                    vec![c + vec2(-4.5, -3.0), c + vec2(4.5, -3.0), c + vec2(0.0, 3.0)],
                    t.icon,
                    Stroke::NONE,
                ));
            }
            resp.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    true,
                    format!("{} {}", crate::i18n::tr(&app.ui.language, "Page"), app.session.page_label(abs)),
                )
            });
            // Parent prefix in the outer top corner (black on white).
            if let Some(pid) = p.parent
                && let Some(pp) = doc.parents.iter().find(|x| x.id == pid)
            {
                let prefix = pp.parent.as_ref().map(|i| i.prefix.clone()).unwrap_or_default();
                let g = ui.painter().layout_no_wrap(prefix, crate::theme::semibold(9.5), Color32::BLACK);
                let w = g.size().x + 2.0;
                let br = if p.side == designcraft_doc::PageSide::Left {
                    egui::Rect::from_min_size(pr.left_top(), vec2(w, 10.0))
                } else {
                    egui::Rect::from_min_size(pr.right_top() - vec2(w, 0.0), vec2(w, 10.0))
                };
                ui.painter().rect_filled(br, 0.0, Color32::WHITE);
                ui.painter().galley(br.min + vec2(1.0, -0.5), g, Color32::BLACK);
            }
            // Drag and drop: pages reorder, parents apply.
            resp.dnd_set_drag_payload(PageDrag::Page(abs));
            if resp.dragged() {
                drag_ghost(ui, &app.session.page_label(abs), &t);
            }
            if dragging && resp.contains_pointer() {
                ui.painter().rect_stroke(pr.expand(1.5), 0.0, Stroke::new(2.0, BADGE), StrokeKind::Outside);
            }
            if let Some(payload) = resp.dnd_release_payload::<PageDrag>() {
                drop = Some(((*payload).clone(), abs));
            }
            if resp.clicked() || resp.double_clicked() {
                if editing_parents {
                    let _ = app.run("layout.parents.edit", json!({"on": false}));
                    app.views.clear();
                }
                crate::canvas::go_to_page(app, abs);
            }
            resp.context_menu(|ui| page_menu(app, ui, &doc, abs, si));
        }
        // The spread's label under its pages: white on a blue badge when it holds the page in view.
        let label = spread_label(app, &doc, first, sp.pages.len());
        let g = ui.painter().layout_no_wrap(label.clone(), crate::theme::semibold(11.0), Color32::WHITE);
        let c = egui::pos2(row.min.x + center, top + THUMB_H + 11.0);
        let br = egui::Rect::from_center_size(c, vec2(g.size().x.max(7.0) + 7.0, 12.5));
        if cur_spread == Some(si) && !editing_parents {
            ui.painter().rect_filled(br, 0.0, BADGE);
        }
        ui.painter().galley(c - g.size() / 2.0, g, Color32::WHITE);
        let lr = ui.interact(br, ui.id().with(("spread_label", si)), Sense::click());
        lr.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &label));
        if lr.clicked() {
            crate::canvas::go_to_page(app, first);
        }
    }
    ui.add_space(6.0);
    ui.spacing_mut().item_spacing.y = 4.0;
    ui.horizontal(|ui| {
        crate::rtl::label(
            ui,
            egui::RichText::new(
                crate::i18n::tr(&app.ui.language, "{pages} Pages in {spreads} Spreads")
                    .replace("{pages}", &doc.page_count().to_string())
                    .replace("{spreads}", &doc.spreads.len().to_string()),
            )
            .size(11.0)
            .color(t.text_dim),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::button(ui, "trash", 20.0, false, crate::i18n::tr(&app.ui.language, "Delete Page")).clicked() {
                let _ = app.run("layout.pages.delete", json!({"pages": [cur]}));
            }
            if icons::button(ui, "plus", 20.0, false, crate::i18n::tr(&app.ui.language, "Insert Page")).clicked() {
                let _ = app.run("layout.pages.insert", json!({"after": cur, "count": 1}));
            }
        });
    });
    match drop {
        Some((PageDrag::Page(from), to)) if from != to => {
            let _ = app.run("layout.pages.move", json!({"from": from, "to": to}));
        }
        Some((PageDrag::Parent(prefix), to)) => {
            let _ = app.run("layout.pages.applyParent", json!({"pages": [to], "parent": prefix}));
        }
        _ => {}
    }
}

/// The panel menu's page commands, acting on the current page and its spread.
pub fn panel_menu(app: &mut DesignApp, ui: &mut egui::Ui) {
    let Some(st) = app.session.active() else { return };
    let doc = st.doc.clone();
    let cur = crate::canvas::current_page(app).unwrap_or(0);
    let Some((si, _)) = doc.page_loc(cur) else { return };
    page_menu(app, ui, &doc, cur, si);
}

fn page_menu(app: &mut DesignApp, ui: &mut egui::Ui, doc: &designcraft_doc::Document, abs: usize, si: usize) {
    if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Insert Page After"))).clicked() {
        let _ = app.run("layout.pages.insert", json!({"after": abs, "count": 1}));
        ui.close();
    }
    if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Delete Page"))).clicked() {
        let _ = app.run("layout.pages.delete", json!({"pages": [abs]}));
        ui.close();
    }
    if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Duplicate Spread"))).clicked() {
        let _ = app.run("layout.pages.duplicateSpread", json!({"spread": si}));
        ui.close();
    }
    if si > 0
        && ui
            .button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Add to Previous Spread")))
            .on_hover_ui(|ui| {
                crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Spreads of up to 10 pages"));
            })
            .clicked()
    {
        let _ = app.run("layout.pages.toSpread", json!({"page": abs + 1, "spread": si - 1}));
        ui.close();
    }
    let mut shuffle = doc.spreads.get(si).is_none_or(|sp| sp.allow_shuffle);
    if ui.checkbox(&mut shuffle, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Allow Spread to Shuffle"))).clicked() {
        let _ = app.run("layout.spreadShuffle", json!({"spread": si, "allow": shuffle}));
        ui.close();
    }
    ui.separator();
    for (id, label) in [
        ("layout.overrideParentItems", "Override All Parent Page Items"),
        ("layout.removeOverrides", "Remove All Local Overrides"),
        ("layout.detachAll", "Detach All Objects from Parent"),
    ] {
        if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, label))).clicked() {
            let _ = app.run(id, json!({"page": abs}));
            ui.close();
        }
    }
    crate::menus::menu_button(ui, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Apply Parent")), |ui| {
        if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "[None]"))).clicked() {
            let _ = app.run("layout.pages.applyParent", json!({"pages": [abs], "parent": null}));
            ui.close();
        }
        for pp in &doc.parents {
            let l = pp.parent.as_ref().map(|i| i.label()).unwrap_or_default();
            if ui.button(&l).clicked() {
                let _ = app.run("layout.pages.applyParent", json!({"pages": [abs], "parent": pp.parent.as_ref().map(|i| i.prefix.clone())}));
                ui.close();
            }
        }
    });
}

/// A small label following the pointer while dragging.
fn drag_ghost(ui: &egui::Ui, label: &str, t: &Tokens) {
    let Some(p) = ui.ctx().pointer_interact_pos() else { return };
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("page_drag")));
    let g = painter.layout_no_wrap(label.to_string(), egui::FontId::proportional(11.0), t.text_strong);
    let r = egui::Rect::from_min_size(p + vec2(10.0, 8.0), g.size() + vec2(10.0, 6.0));
    painter.rect(r, 2.0, t.panel_darker, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
    painter.galley(r.min + vec2(5.0, 3.0), g, t.text_strong);
}

#[cfg(test)]
mod spread_label_tests {
    use egui_kittest::{Harness, kittest::Queryable};
    use serde_json::json;

    use super::*;

    fn app() -> DesignApp {
        let mut app = DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"facingPages": true, "pages": 9})).unwrap();
        app
    }

    fn labels(app: &DesignApp) -> Vec<String> {
        let doc = app.session.doc().unwrap().doc.clone();
        (0..doc.spreads.len()).map(|si| spread_label(app, &doc, doc.first_page_of_spread(si), doc.spreads[si].pages.len())).collect()
    }

    #[test]
    fn spreads_are_labelled_as_indesign_labels_them() {
        // Chris Nielsen's "Numbering Page Sections": sections starting at 1 on pages 3 and 7,
        // the second in letters, label their spreads "2, 1" and "6, A".
        let mut app = app();
        assert_eq!(labels(&app), ["1", "2–3", "4–5", "6–7", "8–9"]);
        app.run("layout.section", json!({"page": 3, "startNumber": 1})).unwrap();
        app.run("layout.section", json!({"page": 7, "startNumber": 1, "style": "upperLetters"})).unwrap();
        assert_eq!(labels(&app), ["1", "2, 1", "2–3", "4, A", "B–C"]);
        // Absolute numbering: positions, always a range.
        app.session.prefs.absolute_page_numbers = true;
        assert_eq!(labels(&app), ["1", "2–3", "4–5", "6–7", "8–9"]);
    }

    #[test]
    fn the_panel_shows_spread_labels_and_a_click_on_one_goes_to_its_spread() {
        let mut app = app();
        app.run("layout.section", json!({"page": 3, "startNumber": 1})).unwrap();
        // The canvas the panel's clicks move (not drawn here).
        app.canvas_rect = Some(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(1000.0, 800.0)));
        let mut h = Harness::builder().with_size(vec2(260.0, 900.0)).build_ui_state(
            |ui, app: &mut DesignApp| {
                let key = egui::Id::new("test_fonts");
                if ui.data(|d| d.get_temp::<bool>(key)).is_none() {
                    crate::theme::install_fonts(ui.ctx(), "");
                    ui.data_mut(|d| d.insert_temp(key, true));
                    return;
                }
                egui::ScrollArea::vertical().show(ui, |ui| show(app, ui));
            },
            app,
        );
        h.run_steps(3);
        let _ = h.get_by_label("2, 1");
        assert!(h.query_by_label("2 1").is_none());
        // Each page is named for screen readers: two pages are numbered 1.
        assert_eq!(h.query_all_by_label("Page 1").count(), 2);
        h.get_by_label("2–3").click();
        h.run_steps(3);
        assert_eq!(h.state().session.doc().unwrap().doc.page_loc(crate::canvas::current_page(h.state()).unwrap()).unwrap().0, 2);
    }
}
