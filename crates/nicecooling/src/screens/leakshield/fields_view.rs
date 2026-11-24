//! The window of the Fields tab: a search field, a filter, and one table of fields.

use gpui::prelude::*;
use gpui::{AnyElement, ClipboardItem, Context, FontWeight, SharedString, Window, div, px};
use gpui_component::{h_flex, v_flex};

use crate::icons::{NcIcon, icon};

use crate::model::FieldRisk;
use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::controls::{Pick, act};
use crate::screens::leakshield::fields::{
    FieldRow, FieldsModel, FieldsState, Filter, Verification,
};
use crate::screens::ui::{
    TagKind, caption, card, cell, label, locked, notice_row, page, table_header, table_row, tag,
};
use crate::tokens::Palette;

/// The widths of the table columns.
const VALUE: f32 = 110.0;
const RISK: f32 = 136.0;
const VERIFICATION: f32 = 196.0;
const ACCESS: f32 = 92.0;

impl ScreenView {
    pub(crate) fn render_ls_fields(
        &mut self,
        id: &str,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = self.with_input(cx, |input| {
            FieldsModel::build(input, &self.leakshield.fields, id)
        });
        let view = cx.entity();
        let search = self.input(
            InputKey::Ls("fields-search".into()),
            &self.leakshield.fields.search.clone(),
            &model.search_placeholder,
            window,
            cx,
        );
        let pick: Pick = std::rc::Rc::new(|this, id, _, _| {
            this.leakshield.fields.filter = match id.as_str() {
                "read-only" => Filter::ReadOnly,
                "safety" => Filter::Safety,
                _ => Filter::All,
            };
        });
        let filter = self.select_field(
            "ls-fields-filter",
            &model.filter_label,
            model
                .filters
                .iter()
                .map(|(filter, text)| {
                    let id = match filter {
                        Filter::All => "all",
                        Filter::ReadOnly => "read-only",
                        Filter::Safety => "safety",
                    };
                    (id.to_owned(), text.clone(), *filter == model.filter)
                })
                .collect(),
            true,
            false,
            p.track,
            pick,
            p,
            cx,
        );
        let copy = model.copy_text.clone();
        let header = h_flex()
            .w_full()
            .gap(px(12.))
            .items_start()
            .child(div().w(px(220.)).flex_none().child(
                Self::field_input(&search, None, false, p).prefix(icon(NcIcon::Search, p.fg_dim)),
            ))
            .child(div().w(px(200.)).flex_none().child(filter))
            .child(
                h_flex().h(px(34.)).flex_1().min_w_0().child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(p.fg_dim)
                        .child(model.summary.clone()),
                ),
            )
            .child(
                Self::small_button(
                    "ls-fields-copy",
                    model.copy_label.clone(),
                    Some(NcIcon::Copy),
                )
                .on_click(act(&view, move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                })),
            );

        let mut content = page().child(header);
        match model.state {
            FieldsState::Loading => {
                content = content.child(
                    card(p)
                        .p(px(18.))
                        .gap(px(12.))
                        .child(skeleton(p, 220.))
                        .child(skeleton(p, 520.)),
                );
            }
            FieldsState::UnknownFirmware => {
                if let Some(text) = &model.unknown_text {
                    content = content.child(card(p).child(notice_row(text.clone(), true, p)));
                }
            }
            FieldsState::Empty => {
                content = content.child(
                    card(p)
                        .p(px(18.))
                        .child(model.empty.clone().unwrap_or_default()),
                );
            }
            FieldsState::Ready => {
                let mut table = card(p).child(
                    table_header(
                        &model.columns,
                        &[
                            None,
                            Some(VALUE),
                            Some(RISK),
                            Some(VERIFICATION),
                            Some(ACCESS),
                        ],
                        p,
                    )
                    .border_t_0(),
                );
                for group in &model.groups {
                    // A group row: the name of the setting group on the `hover` ground.
                    table = table.child(
                        h_flex()
                            .w_full()
                            .h(px(30.))
                            .px(px(18.))
                            .border_t_1()
                            .border_color(p.border)
                            .bg(p.hover)
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.fg_dim)
                            .child(group.title.clone()),
                    );
                    for row in &group.rows {
                        table = table.child(self.field_row(row, model.dim, p, cx));
                    }
                }
                content = content.child(table);
            }
        }
        content = content.child(
            v_flex()
                .px(px(6.))
                .gap(px(3.))
                .child(caption(model.risk_help.clone(), p))
                .children(
                    model
                        .verification_help
                        .iter()
                        .map(|text| caption(text.clone(), p)),
                ),
        );

        let ids = model.row_ids();
        let mut root = div().id("ls-fields").w_full().child(content);
        if let Some(focus) = self.ls_focus.clone() {
            root = root.track_focus(&focus).on_key_down(move |event, _, cx| {
                let delta = match event.keystroke.key.as_str() {
                    "down" => 1,
                    "up" => -1,
                    _ => return,
                };
                let ids = ids.clone();
                view.update(cx, |this, cx| {
                    this.leakshield.fields.step(&ids, delta);
                    cx.notify();
                });
            });
        }
        root.into_any_element()
    }

    fn field_row(
        &mut self,
        row: &FieldRow,
        dim: bool,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let id = row.id.clone();
        let ink = if dim { p.fg_dim } else { p.fg };
        let (badge, badge_color) = match row.verification {
            Verification::VerificationWritePassed => (NcIcon::ShieldOk, p.success),
            Verification::PlausibleRead => (NcIcon::Eye, p.fg_dim),
            Verification::CodeDefined => (NcIcon::Code, p.fg_dim),
        };
        let line = table_row(p)
            .id(SharedString::from(format!("ls-field-{}", row.id)))
            .min_h(px(48.))
            .text_color(ink)
            .cursor_pointer()
            .when(row.selected, |line| line.bg(p.accent_tint))
            .when(!row.selected, |line| line.hover(|line| line.bg(p.hover)))
            .on_click({
                let focus = self.ls_focus.clone();
                move |_, window, cx| {
                    let id = id.clone();
                    if let Some(focus) = &focus {
                        window.focus(focus, cx);
                    }
                    view.update(cx, |this, cx| {
                        this.leakshield.fields.select(&id);
                        cx.notify();
                    });
                }
            })
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(label(row.name.clone()))
                    .child(
                        div()
                            .truncate()
                            .font_family("monospace")
                            .text_size(px(12.))
                            .line_height(px(16.))
                            // Never `fg-dim` on a tint.
                            .text_color(if row.selected { p.fg } else { p.fg_dim })
                            .child(row.id.clone()),
                    ),
            )
            .child(cell(VALUE).child(row.value.clone()))
            .child(div().w(px(RISK)).flex_none().child(tag(
                row.risk_text.clone(),
                if row.risk == FieldRisk::Safety {
                    TagKind::Warning
                } else {
                    TagKind::Neutral
                },
                p,
            )))
            .child(
                h_flex()
                    .w(px(VERIFICATION))
                    .flex_none()
                    .min_w_0()
                    .gap(px(4.))
                    .text_size(px(12.))
                    .text_color(badge_color)
                    .child(icon(badge, badge_color))
                    .child(label(row.verification_text.clone())),
            )
            .child(cell(ACCESS).map(|access| {
                if row.writable {
                    access.child(row.access_text.clone())
                } else {
                    access
                        .text_color(p.fg_dim)
                        .child(locked(row.access_text.clone(), p))
                }
            }));
        let mut column = v_flex().w_full().child(line);
        if let Some(text) = &row.explanation {
            column = column.child(
                h_flex()
                    .w_full()
                    .px(px(18.))
                    .py(px(9.))
                    .gap(px(9.))
                    .items_start()
                    .bg(p.hover)
                    .child(icon(NcIcon::Info, p.fg_dim))
                    .child(div().flex_1().min_w_0().child(text.clone())),
            );
        }
        column.into_any_element()
    }
}
