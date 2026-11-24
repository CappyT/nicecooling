//! The window of the Fields tab: a search field, a filter, and one table of fields.

use gpui::prelude::*;
use gpui::{AnyElement, ClipboardItem, Context, FontWeight, SharedString, Window, div, px};
use gpui_component::input::Input;
use gpui_component::{h_flex, v_flex};

use crate::model::FieldRisk;
use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::controls::{Pick, act};
use crate::screens::leakshield::fields::{
    FieldRow, FieldsModel, FieldsState, Filter, Verification,
};
use crate::screens::ui::{
    TagKind, caption, card, card_header, card_row, locked, notice_row, page, tag,
};
use crate::tokens::Palette;

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
        let filter = self.select(
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
            "",
            pick,
            p,
            cx,
        );
        let copy = model.copy_text.clone();
        let header = h_flex()
            .gap(px(12.))
            .items_center()
            .child(div().w(px(260.)).child(Input::new(&search)))
            .child(filter)
            .child(
                div()
                    .flex_1()
                    .text_color(p.fg_dim)
                    .child(model.summary.clone()),
            )
            .child(
                Self::flat_button("ls-fields-copy", model.copy_label.clone())
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
                let mut table = card(p).child(columns(&model.columns, p));
                for group in &model.groups {
                    table = table.child(
                        card_header(group.title.clone(), p)
                            .bg(p.selected)
                            .h(px(30.)),
                    );
                    for row in &group.rows {
                        table = table.child(self.field_row(row, model.dim, p, cx));
                    }
                }
                content = content.child(table);
            }
        }
        content = content.child(caption(model.risk_help.clone(), p)).children(
            model
                .verification_help
                .iter()
                .map(|text| caption(text.clone(), p)),
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
        let verified = row.verification == Verification::VerificationWritePassed;
        let line = h_flex()
            .id(SharedString::from(format!("ls-field-{}", row.id)))
            .w_full()
            .min_h(px(40.))
            .px(px(18.))
            .py(px(4.))
            .gap(px(12.))
            .items_center()
            .border_t_1()
            .border_color(p.border)
            .text_color(ink)
            .cursor_pointer()
            .when(row.selected, |line| line.bg(p.accent_tint))
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
                v_flex().flex_1().child(row.name.clone()).child(
                    div()
                        .font_family("monospace")
                        .text_size(px(11.))
                        .text_color(p.fg_dim)
                        .child(row.id.clone()),
                ),
            )
            .child(div().w(px(180.)).child(row.value.clone()))
            .child(div().w(px(110.)).child(tag(
                row.risk_text.clone(),
                if row.risk == FieldRisk::Safety {
                    TagKind::Warning
                } else {
                    TagKind::Neutral
                },
                p,
            )))
            .child(
                div()
                    .w(px(200.))
                    .text_size(px(12.))
                    .text_color(if verified { p.success } else { p.fg_dim })
                    .child(row.verification_text.clone()),
            )
            .child(
                div()
                    .w(px(90.))
                    .font_weight(FontWeight::BOLD)
                    .map(|access| {
                        if row.writable {
                            access.child(row.access_text.clone())
                        } else {
                            access.child(locked(row.access_text.clone(), p))
                        }
                    }),
            );
        let mut column = v_flex().w_full().child(line);
        if let Some(text) = &row.explanation {
            column = column.child(card_row(p).bg(p.selected).child(text.clone()));
        }
        column.into_any_element()
    }
}

fn columns(names: &[String], p: &Palette) -> AnyElement {
    let widths = [None, Some(180.), Some(110.), Some(200.), Some(90.)];
    let mut line = h_flex()
        .h(px(36.))
        .px(px(18.))
        .gap(px(12.))
        .items_center()
        .text_size(px(12.))
        .font_weight(FontWeight::BOLD)
        .text_color(p.fg_dim);
    for (name, width) in names.iter().zip(widths) {
        line = line.child(match width {
            Some(width) => div().w(px(width)).child(name.clone()),
            None => div().flex_1().child(name.clone()),
        });
    }
    line.into_any_element()
}
