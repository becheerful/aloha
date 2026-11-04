rust_i18n::i18n!("locales");

use std::path::Path;

use eframe::egui::{self, Ui};
use modern_egui::theme::UiButtons;
use rust_i18n::t;

mod utils;

struct AlohaViewer {
    locale: &'static str,
    file_name: String,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
    zoom: f32,
    auto_zoom: bool,
    pending_input: Option<std::path::PathBuf>,
}

impl AlohaViewer {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        modern_egui::theme::apply(&cc.egui_ctx);

        Self {
            locale: "en",
            file_name: t!("file-not-selected", locale = "en").into_owned(),
            texture: None,
            error: None,
            zoom: 1.0,
            auto_zoom: true,
            pending_input: None,
        }
    }

    fn load(&mut self, ui: &mut Ui, path: &Path) {
        let file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };

        match aloha_core::AlohaImage::read_from(std::io::BufReader::new(file)) {
            Ok(image) => match utils::aloha_to_rgba8(&image) {
                Ok(data) => {
                    let color = egui::ColorImage::from_rgba_unmultiplied(
                        [image.width as usize, image.height as usize],
                        &data,
                    );

                    match &mut self.texture {
                        Some(handle) => {
                            handle.set(color, egui::TextureOptions::default());
                        }

                        None => {
                            self.texture = Some(ui.load_texture(
                                "image",
                                color,
                                egui::TextureOptions::NEAREST,
                            ));
                        }
                    }

                    self.file_name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("?")
                        .to_owned();
                }

                Err(e) => {
                    self.error = Some(e);
                }
            },

            Err(e) => {
                self.error = Some(e.to_string());
            }
        }
    }

    fn convert_to(&mut self, ui: &mut Ui, path: &Path) {
        let default_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image")
            .to_string()
            + ".aloha";

        let Some(out) = rfd::FileDialog::new()
            .add_filter(t!("aloha-images", locale = self.locale), &["aloha"])
            .set_file_name(&default_name)
            .save_file()
        else {
            return;
        };

        match path.extension().and_then(|s| s.to_str()).unwrap() {
            "png" => {
                let _ = aloha_convert::png_to_aloha(&path, &out)
                    .map_err(|e| self.error = Some(e.to_string()));
            }

            "jpg" | "jpeg" => {
                let _ = aloha_convert::jpeg_to_aloha(&path, &out)
                    .map_err(|e| self.error = Some(e.to_string()));
            }

            _ => unreachable!(),
        };

        self.load(ui, &out);
    }

    fn convert_from(&mut self, ui: &mut Ui, path: &Path, ext: &str) {
        self.load(ui, path);

        let default_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image")
            .to_string()
            + "."
            + ext;

        let Some(out) = rfd::FileDialog::new()
            .add_filter(t!("images", locale = self.locale), &[ext])
            .set_file_name(&default_name)
            .save_file()
        else {
            return;
        };

        match ext {
            "png" => {
                let _ = aloha_convert::aloha_to_png(&path, &out)
                    .map_err(|e| self.error = Some(e.to_string()));
            }

            "jpg" | "jpeg" => {
                let _ = aloha_convert::aloha_to_jpeg(&path, &out)
                    .map_err(|e| self.error = Some(e.to_string()));
            }

            _ => unreachable!(),
        };
    }
}

impl eframe::App for AlohaViewer {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        egui::MenuBar::default().ui(ui, |ui| {
            ui.menu_button(t!("file", locale = self.locale), |ui| {
                if ui.button(t!("open-file", locale = self.locale)).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter(t!("aloha-images", locale = self.locale), &["aloha"])
                        .pick_file()
                    {
                        self.load(ui, &path);
                    }
                }

                ui.menu_button(t!("convert", locale = self.locale), |ui| {
                    if ui.button(t!("in-aloha", locale = self.locale)).clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter(t!("images", locale = self.locale), &["png", "jpg", "jpeg"])
                            .pick_file()
                        {
                            self.convert_to(ui, &path);
                        }
                    }

                    if ui.button(t!("from-aloha", locale = self.locale)).clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter(t!("aloha-images", locale = self.locale), &["aloha"])
                            .pick_file()
                        {
                            self.pending_input = Some(path);
                        }
                    }
                });

                if ui.danger_button(t!("exit", locale = self.locale)).clicked() {
                    let ctx = ui.ctx().clone();
                    std::thread::spawn(move || {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    });
                }
            });

            ui.menu_button(t!("language", locale = self.locale), |ui| {
                if ui.button("English").clicked() {
                    self.locale = "en";
                }

                if ui.button("Русский").clicked() {
                    self.locale = "ru";
                }
            });

            if self.texture.is_some() {
                ui.separator();
                ui.label(t!("scale", locale = self.locale));
                if ui
                    .add(egui::Slider::new(&mut self.zoom, 0.1..=16.0).logarithmic(true))
                    .changed()
                {
                    self.auto_zoom = false;
                }

                if ui.button("1:1").clicked() {
                    self.zoom = 1.0;
                    self.auto_zoom = false;
                }

                ui.checkbox(&mut self.auto_zoom, t!("auto", locale = self.locale));
            }
        });

        egui::Panel::top("top_pannel").show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(match &self.texture {
                    Some(t) => {
                        let size = t.size();
                        format!("{} ({}x{})", self.file_name, size[0], size[1])
                    }

                    None => t!("file-not-selected", locale = self.locale).into_owned(),
                });
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(texture) = &self.texture {
                let avail = ui.available_size();
                let size = texture.size_vec2();

                let scale = if self.auto_zoom {
                    (avail.x / size.x).min(avail.y / size.y)
                } else {
                    self.zoom
                };

                ui.centered_and_justified(|ui| {
                    ui.add(egui::Image::new(texture).fit_to_exact_size(size * scale));
                });
                self.zoom = scale;
            }

            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::RED, error);
            }
        });

        if let Some(path) = self.pending_input.clone() {
            let mut close = false;
            egui::Window::new(t!("select-format", locale = self.locale))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui, |ui| {
                    ui.label(t!(
                        "convert-in",
                        locale = self.locale,
                        name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?")
                    ));

                    ui.add_space(4.0);

                    ui.horizontal(|ui| {
                        if ui.button("PNG").clicked() {
                            self.convert_from(ui, &path, "png");
                            close = true;
                        }

                        if ui.button("JPEG").clicked() {
                            self.convert_from(ui, &path, "jpg");
                            close = true;
                        }

                        if ui
                            .danger_button(t!("cancel", locale = self.locale))
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });

            if close {
                self.pending_input = None;
            }
        }
    }
}

fn main() -> eframe::Result {
    let mut options = eframe::NativeOptions::default();
    options.viewport.min_inner_size = Some(egui::Vec2::new(640.0, 480.0));

    eframe::run_native(
        "ALOHA Viewer",
        options,
        Box::new(|cc| Ok(Box::new(AlohaViewer::new(cc)))),
    )
}
