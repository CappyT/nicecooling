//! The window of the System tab: six cards.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, px};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::progress::Progress;
use gpui_component::switch::Switch;
use gpui_component::{Disableable as _, Sizable as _, h_flex};

use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::controls::{Pick, act};
use crate::screens::leakshield::TabState;
use crate::screens::leakshield::sensors_view::header_row;
use crate::screens::leakshield::system::{Ask, SystemModel, direct_fields};
use crate::screens::ui::{
    TagKind, caption, card, card_header, card_row, modal, notice_row, page, tag, write_line,
};
use crate::tokens::Palette;

impl ScreenView {
    pub(crate) fn render_ls_system(
        &mut self,
        id: &str,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = self.with_input(cx, |input| {
            SystemModel::build(input, &self.leakshield.system, id)
        });
        let view = cx.entity();
        let mut content = page();
        if model.state == TabState::Loading {
            return content
                .child(card(p).p(px(18.)).gap(px(12.)).child(skeleton(p, 300.)))
                .into_any_element();
        }

        // Device.
        let device = &model.device;
        let mut card_device = card(p)
            .child(card_header(device.title.clone(), p))
            .child(
                card_row(p)
                    .child(
                        div()
                            .w(px(180.))
                            .font_weight(FontWeight::BOLD)
                            .child(device.serial_label.clone()),
                    )
                    .child(div().font_family("monospace").child(device.serial.clone())),
            )
            .child(
                card_row(p)
                    .child(
                        div()
                            .w(px(180.))
                            .font_weight(FontWeight::BOLD)
                            .child(device.firmware_label.clone()),
                    )
                    .child(div().mr(px(8.)).child(device.firmware.clone()))
                    .child(tag(
                        device.firmware_tag.clone(),
                        if device.firmware_known {
                            TagKind::Neutral
                        } else {
                            TagKind::Warning
                        },
                        p,
                    )),
            );
        if let Some(text) = &device.unknown_text {
            card_device = card_device.child(notice_row(text.clone(), true, p));
        }
        card_device = card_device.child(card_row(p).child(caption(device.update_hint.clone(), p)));
        let counter = {
            let ask = view.clone();
            let button = &device.reset_counter;
            Button::new("ls-reset-counter")
                .small()
                .label(button.label.clone())
                .disabled(!button.enabled)
                .on_click(act(&ask, |this, _, _| {
                    this.leakshield.system.request(Ask::ServiceCounter)
                }))
        };
        card_device = card_device.child(
            card_row(p)
                .child(
                    div()
                        .w(px(180.))
                        .font_weight(FontWeight::BOLD)
                        .child(device.maintenance_label.clone()),
                )
                .child(div().w(px(160.)).child(
                    Progress::new("ls-maintenance").value(device.maintenance.unwrap_or(0.) as f32),
                ))
                .child(div().w(px(60.)).child(device.maintenance_text.clone()))
                .child(div().flex_1())
                .child(counter),
        );
        if let Some(text) = &device.maintenance_warning {
            card_device = card_device.child(notice_row(text.clone(), true, p));
        }
        card_device =
            card_device.child(card_row(p).child(caption(device.maintenance_caption.clone(), p)));
        if let Some(text) = &device.counter_unsupported {
            card_device = card_device.child(card_row(p).child(caption(text.clone(), p)));
        }
        if let Some(line) = &device.counter_line {
            card_device = card_device.child(card_row(p).child(write_line(line, p)));
        }
        content = content.child(card_device);

        // Clock.
        let clock = &model.clock;
        let zone_pick: Pick = {
            let device = id.to_owned();
            Rc::new(move |this, hours, _, _| {
                if let Ok(hours) = hours.parse::<f64>() {
                    this.send(direct_fields(&device, "clock", "clock_timezone", hours));
                }
            })
        };
        let zone_select = self.select(
            "ls-zone",
            &clock.zone_text,
            clock
                .zone_options
                .iter()
                .map(|(hours, text)| (hours.to_string(), text.clone(), Some(*hours) == clock.zone))
                .collect(),
            clock.zone_writable,
            "",
            zone_pick,
            p,
            cx,
        );
        let format_pick: Pick = {
            let device = id.to_owned();
            Rc::new(move |this, choice, _, cx| {
                let flags = this
                    .state(cx)
                    .snapshot
                    .leakshield_data
                    .get(&device)
                    .and_then(|d| d.number("clock_flags"))
                    .unwrap_or(0.0) as u32;
                let flags = if choice == "12" {
                    flags | 2
                } else {
                    flags & !2
                };
                this.send(direct_fields(
                    &device,
                    "clock",
                    "clock_flags",
                    f64::from(flags),
                ));
            })
        };
        let format_select = self.select(
            "ls-time-format",
            clock
                .format_options
                .iter()
                .find(|(hour12, _)| *hour12 == clock.hour12)
                .map_or("", |(_, text)| text.as_str()),
            clock
                .format_options
                .iter()
                .map(|(hour12, text)| {
                    (
                        if *hour12 { "12" } else { "24" }.to_owned(),
                        text.clone(),
                        *hour12 == clock.hour12,
                    )
                })
                .collect(),
            clock.format_writable,
            "",
            format_pick,
            p,
            cx,
        );
        let sync = {
            let device = id.to_owned();
            Button::new("ls-sync-clock")
                .small()
                .label(clock.sync.label.clone())
                .disabled(!clock.sync.enabled)
                .on_click(act(&view, move |this, _, _| {
                    this.send(crate::command::Command::SyncLeakshieldClock {
                        leakshield: device.clone(),
                    });
                }))
        };
        let mut clock_card = card(p)
            .child(card_header(clock.title.clone(), p).child(sync))
            .child(
                card_row(p)
                    .child(
                        div()
                            .w(px(180.))
                            .font_weight(FontWeight::BOLD)
                            .child(clock.time_label.clone()),
                    )
                    .child(div().child(clock.time.clone())),
            )
            .child(
                card_row(p)
                    .child(
                        div()
                            .w(px(180.))
                            .font_weight(FontWeight::BOLD)
                            .child(clock.zone_label.clone()),
                    )
                    .child(zone_select),
            )
            .child(
                card_row(p)
                    .child(
                        div()
                            .w(px(180.))
                            .font_weight(FontWeight::BOLD)
                            .child(clock.format_label.clone()),
                    )
                    .child(format_select),
            );
        if let Some(text) = &clock.warning {
            clock_card = clock_card.child(notice_row(text.clone(), true, p));
        }
        if let Some(line) = &clock.sync_line {
            clock_card = clock_card.child(card_row(p).child(write_line(line, p)));
        }
        content = content.child(clock_card);

        // Standby.
        let mut standby = card(p)
            .child(card_header(model.standby.title.clone(), p))
            .child(card_row(p).child(caption(model.standby.text.clone(), p)));
        for row in &model.standby.rows {
            let (device, bit) = (id.to_owned(), row.bit);
            let view = view.clone();
            standby = standby.child(
                card_row(p)
                    .child(div().flex_1().child(row.label.clone()))
                    .child(
                        Switch::new(SharedString::from(format!("ls-standby-{bit}")))
                            .checked(row.on)
                            .disabled(!row.writable)
                            .on_click(move |on, _, cx| {
                                let on = *on;
                                let device = device.clone();
                                view.update(cx, |this, cx| {
                                    let mask = this
                                        .state(cx)
                                        .snapshot
                                        .leakshield_data
                                        .get(&device)
                                        .and_then(|d| d.number("standby_conditions"))
                                        .unwrap_or(0.0)
                                        as u32;
                                    let mask = if on { mask | bit } else { mask & !bit };
                                    this.send(direct_fields(
                                        &device,
                                        "standby",
                                        "standby_conditions",
                                        f64::from(mask),
                                    ));
                                });
                            }),
                    ),
            );
        }
        if let Some(line) = &model.standby.write_line {
            standby = standby.child(card_row(p).child(write_line(line, p)));
        }
        content = content.child(standby);

        // Backups.
        let backups = &model.backups;
        let back_up = {
            let device = id.to_owned();
            Button::new("ls-back-up")
                .small()
                .label(backups.back_up.label.clone())
                .disabled(!backups.back_up.enabled)
                .on_click(act(&view, move |this, _, _| {
                    this.send(crate::command::Command::CreateBackup {
                        leakshield: device.clone(),
                    });
                }))
        };
        let import = {
            let device = id.to_owned();
            Button::new("ls-import")
                .small()
                .label(backups.import.label.clone())
                .disabled(!backups.import.enabled)
                .on_click(act(&view, move |this, _, cx| {
                    this.pick_backup_file(&device, cx);
                }))
        };
        let mut backups_card = card(p)
            .child(
                card_header(backups.title.clone(), p)
                    .child(back_up)
                    .child(import),
            )
            .child(header_row(&backups.columns, &[170., 160., 0., 200.], p));
        if let Some(text) = &backups.empty {
            backups_card = backups_card.child(card_row(p).child(text.clone()));
        }
        for row in &backups.rows {
            let (backup, device) = (row.id.clone(), id.to_owned());
            let restore_id = backup.clone();
            let export = Button::new(SharedString::from(format!("ls-export-{}", row.id)))
                .small()
                .label(row.export.label.clone())
                .disabled(!row.export.enabled)
                .on_click(act(&view, {
                    let device = device.clone();
                    move |this, _, cx| this.save_backup_file(&device, &backup, cx)
                }));
            let restore = Button::new(SharedString::from(format!("ls-restore-{}", row.id)))
                .small()
                .danger()
                .label(row.restore.label.clone())
                .disabled(!row.restore.enabled)
                .on_click(act(&view, move |this, _, _| {
                    this.leakshield
                        .system
                        .request(Ask::Restore(restore_id.clone()))
                }));
            backups_card = backups_card.child(
                card_row(p)
                    .child(div().w(px(170.)).child(row.time.clone()))
                    .child(div().w(px(160.)).child(row.reason.clone()))
                    .child(
                        div()
                            .flex_1()
                            .font_family("monospace")
                            .text_color(p.fg_dim)
                            .child(row.short_hash.clone()),
                    )
                    .child(
                        h_flex()
                            .w(px(200.))
                            .gap(px(6.))
                            .justify_end()
                            .child(export)
                            .child(restore),
                    ),
            );
        }
        backups_card = backups_card.child(card_row(p).child(caption(backups.caption.clone(), p)));
        if backups.restoring {
            backups_card = backups_card.child(card_row(p).child(caption(
                self.model.read(cx).locale.tr("feedback.pending"),
                p,
            )));
        }
        if let Some(failed) = &backups.restore_failed {
            let (retry, close, device) = (view.clone(), view.clone(), id.to_owned());
            backups_card = backups_card.child(
                card_row(p)
                    .bg(p.alarm_tint)
                    .child(div().flex_1().child(failed.text.clone()))
                    .child(
                        Button::new("ls-restore-retry")
                            .small()
                            .label(failed.try_again.clone())
                            .on_click(act(&retry, move |this, _, _| {
                                if let Some(command) = this.leakshield.system.retry_restore(&device)
                                {
                                    this.send(command);
                                }
                            })),
                    )
                    .child(
                        Button::new("ls-restore-close")
                            .small()
                            .label(failed.close.clone())
                            .on_click(act(&close, |this, _, _| {
                                this.leakshield.system.close_failure()
                            })),
                    ),
            );
        }
        if let Some(line) = &backups.write_line {
            backups_card = backups_card.child(card_row(p).child(write_line(line, p)));
        }
        content = content.child(backups_card);

        // Refill history.
        let refill = &model.refill;
        let reset_history = Button::new("ls-reset-history")
            .small()
            .label(refill.reset.label.clone())
            .disabled(!refill.reset.enabled)
            .on_click(act(&view, |this, _, _| {
                this.leakshield.system.request(Ask::RefillHistory)
            }));
        let mut refill_card = card(p)
            .child(card_header(refill.title.clone(), p).child(reset_history))
            .child(header_row(&refill.columns, &[170., 160., 0.], p))
            .child(card_row(p).child(refill.empty.clone()));
        if let Some(text) = &refill.unsupported {
            refill_card = refill_card.child(card_row(p).child(caption(text.clone(), p)));
        }
        if let Some(line) = &refill.line {
            refill_card = refill_card.child(card_row(p).child(write_line(line, p)));
        }
        content = content.child(refill_card);

        // Reset.
        let reset = &model.reset;
        let mut reset_card = card(p).child(card_header(reset.title.clone(), p)).child(
            card_row(p).child(
                Button::new("ls-factory-reset")
                    .small()
                    .danger()
                    .label(reset.factory.label.clone())
                    .disabled(!reset.factory.enabled)
                    .on_click(act(&view, |this, _, _| {
                        this.leakshield.system.request(Ask::FactoryReset)
                    })),
            ),
        );
        if let Some(text) = &reset.denied {
            reset_card = reset_card.child(card_row(p).text_color(p.alarm).child(text.clone()));
        } else if let Some(line) = &reset.write_line {
            reset_card = reset_card.child(card_row(p).child(write_line(line, p)));
        }
        content = content.child(reset_card);

        let mut root = div().relative().w_full().child(content);
        if let Some(dialog) = &model.dialog {
            let mut body = Vec::new();
            if let Some(word) = &dialog.word {
                let input = self.input(
                    InputKey::Ls("reset-word".into()),
                    &self.leakshield.system.reset_word.clone(),
                    word,
                    window,
                    cx,
                );
                body.push(Input::new(&input).into_any_element());
            }
            let (cancel, confirm, device) = (view.clone(), view.clone(), id.to_owned());
            root = root.child(modal(
                dialog.title.clone(),
                dialog.text.clone(),
                body,
                vec![
                    Button::new("ls-ask-cancel")
                        .small()
                        .label(dialog.cancel.clone())
                        .on_click(act(&cancel, |this, _, _| this.leakshield.system.cancel()))
                        .into_any_element(),
                    Button::new("ls-ask-confirm")
                        .small()
                        .danger()
                        .label(dialog.confirm.clone())
                        .disabled(!dialog.confirm_enabled)
                        .on_click(act(&confirm, move |this, _, _| {
                            if let Some(command) = this.leakshield.system.confirm(&device) {
                                this.send(command);
                            }
                        }))
                        .into_any_element(),
                ],
                p,
            ));
        }
        root.into_any_element()
    }

    /// "Export": the file dialog of the system, then the command that writes the file.
    fn save_backup_file(&mut self, device: &str, backup: &str, cx: &mut Context<Self>) {
        let (device, backup) = (device.to_owned(), backup.to_owned());
        let commands = self.commands.clone();
        let path = cx.prompt_for_new_path(
            &std::env::temp_dir(),
            Some(&format!("leakshield-{backup}.txt")),
        );
        cx.spawn(async move |_, _| {
            if let Ok(Ok(Some(path))) = path.await {
                let _ = commands.send(crate::command::Command::ExportBackup {
                    leakshield: device,
                    backup,
                    path,
                });
            }
        })
        .detach();
    }

    /// "Import": the file dialog of the system, then the command that reads the file.
    fn pick_backup_file(&mut self, device: &str, cx: &mut Context<Self>) {
        let device = device.to_owned();
        let commands = self.commands.clone();
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |_, _| {
            if let Ok(Ok(Some(mut paths))) = paths.await
                && let Some(path) = paths.pop()
            {
                let _ = commands.send(crate::command::Command::ImportBackup {
                    leakshield: device,
                    path,
                });
            }
        })
        .detach();
    }
}
