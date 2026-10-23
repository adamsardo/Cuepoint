//! Two-channel mixer panel: trim, EQ, meters, channel faders, crossfader
//! and master (spec 05-ui, section 2.8).

use cuepoint_engine::{
    Command, CrossfaderAssign, EQ_MAX_DB, EQ_MIN_DB, EqBand, MixerState, TRIM_MAX_DB, TRIM_MIN_DB,
};
use egui::{Color32, Rect, RichText, Sense, Ui, Vec2};

/// Meter fall rate in dB per second.
const METER_FALL_DB_PER_SEC: f32 = 24.0;
const METER_FLOOR_DB: f32 = -48.0;

#[derive(Debug, Clone)]
pub struct MixerView {
    trim_db: [f32; 2],
    /// [channel][low, mid, high]
    eq_db: [[f32; 3]; 2],
    fader: [f32; 2],
    assign: [CrossfaderAssign; 2],
    crossfader: f32,
    master: f32,
    /// Displayed meter levels in dB, with fall-back.
    meter_db: [f32; 2],
}

impl Default for MixerView {
    fn default() -> Self {
        Self {
            trim_db: [0.0; 2],
            eq_db: [[0.0; 3]; 2],
            fader: [1.0; 2],
            assign: [CrossfaderAssign::A, CrossfaderAssign::B],
            crossfader: 0.0,
            master: 1.0,
            meter_db: [METER_FLOOR_DB; 2],
        }
    }
}

impl MixerView {
    /// True while a meter is still falling back, so the UI must keep
    /// redrawing for the fall to look smooth.
    pub fn is_settling(&self) -> bool {
        self.meter_db.iter().any(|&db| db > METER_FLOOR_DB)
    }

    pub fn show(&mut self, ui: &mut Ui, state: &MixerState, commands: &mut Vec<Command>) {
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        for (shown, &peak) in self.meter_db.iter_mut().zip(&state.channel_peak) {
            let db = if peak > 0.0 {
                20.0 * peak.log10()
            } else {
                METER_FLOOR_DB
            };
            *shown = db
                .max(*shown - METER_FALL_DB_PER_SEC * dt)
                .max(METER_FLOOR_DB);
        }
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("MIXER").strong().monospace());
        });
        ui.columns(2, |columns| {
            for (channel, ui) in columns.iter_mut().enumerate() {
                self.channel_strip(ui, channel, commands);
            }
        });
        ui.horizontal(|ui| {
            ui.label("A");
            let crossfader = egui::Slider::new(&mut self.crossfader, -1.0..=1.0).show_value(false);
            if ui
                .add_sized(Vec2::new(ui.available_width() - 20.0, 18.0), crossfader)
                .changed()
            {
                commands.push(Command::SetCrossfader {
                    position: self.crossfader,
                });
            }
            ui.label("B");
        });
        ui.horizontal(|ui| {
            ui.label("MASTER");
            if ui
                .add(egui::Slider::new(&mut self.master, 0.0..=1.0).show_value(false))
                .changed()
            {
                commands.push(Command::SetMaster {
                    position: self.master,
                });
            }
        });
    }

    fn channel_strip(&mut self, ui: &mut Ui, channel: usize, commands: &mut Vec<Command>) {
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(format!("CH {}", channel + 1))
                    .small()
                    .strong(),
            );
            let knob = |ui: &mut Ui, label: &str, value: &mut f32, min: f32, max: f32| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).small().monospace());
                    let response = ui.add(egui::Slider::new(value, min..=max).show_value(false));
                    if response.double_clicked() {
                        *value = 0.0;
                    }
                    response.changed() || response.double_clicked()
                })
                .inner
            };
            if knob(
                ui,
                "TRIM",
                &mut self.trim_db[channel],
                TRIM_MIN_DB,
                TRIM_MAX_DB,
            ) {
                commands.push(Command::SetTrimDb {
                    channel,
                    db: self.trim_db[channel],
                });
            }
            for (band, label) in [
                (EqBand::High, "HI  "),
                (EqBand::Mid, "MID "),
                (EqBand::Low, "LOW "),
            ] {
                let index = match band {
                    EqBand::Low => 0,
                    EqBand::Mid => 1,
                    EqBand::High => 2,
                };
                if knob(
                    ui,
                    label,
                    &mut self.eq_db[channel][index],
                    EQ_MIN_DB,
                    EQ_MAX_DB,
                ) {
                    commands.push(Command::SetEqDb {
                        channel,
                        band,
                        db: self.eq_db[channel][index],
                    });
                }
            }
            ui.horizontal(|ui| {
                self.meter(ui, channel);
                let fader = egui::Slider::new(&mut self.fader[channel], 0.0..=1.0)
                    .vertical()
                    .show_value(false);
                if ui.add_sized(Vec2::new(24.0, 90.0), fader).changed() {
                    commands.push(Command::SetFader {
                        channel,
                        position: self.fader[channel],
                    });
                }
            });
            ui.horizontal(|ui| {
                for (assign, label) in [
                    (CrossfaderAssign::A, "A"),
                    (CrossfaderAssign::Thru, "THRU"),
                    (CrossfaderAssign::B, "B"),
                ] {
                    if ui
                        .selectable_label(
                            self.assign[channel] == assign,
                            RichText::new(label).small(),
                        )
                        .clicked()
                    {
                        self.assign[channel] = assign;
                        commands.push(Command::SetAssign { channel, assign });
                    }
                }
            });
        });
    }

    fn meter(&self, ui: &mut Ui, channel: usize) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 90.0), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 1.0, ui.visuals().extreme_bg_color);
        let level = ((self.meter_db[channel] - METER_FLOOR_DB) / -METER_FLOOR_DB).clamp(0.0, 1.0);
        let top = rect.bottom() - level * rect.height();
        let colour = if self.meter_db[channel] > -1.0 {
            Color32::from_rgb(230, 60, 50)
        } else if self.meter_db[channel] > -9.0 {
            Color32::from_rgb(230, 190, 40)
        } else {
            Color32::from_rgb(60, 200, 90)
        };
        painter.rect_filled(
            Rect::from_x_y_ranges(rect.x_range(), top..=rect.bottom()),
            1.0,
            colour,
        );
    }
}
