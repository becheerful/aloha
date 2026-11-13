rust_i18n::i18n!("locales");

use std::path::Path;

use eframe::egui::{self, Ui};
use rust_i18n::t;

use crate::utils::xmp_value_to_string;

mod utils;

struct AlohaViewer {
    locale: &'static str,
    file_name: String,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
    zoom: f32,
    auto_zoom: bool,
    pending_input: Option<std::path::PathBuf>,
    convert_format: &'static str,
    show_metadata: bool,
    metadata: Option<aloha_core::metadata::Metadata>,
    image_info: Option<utils::ImageInfo>,
}

impl AlohaViewer {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);

        Self {
            locale: "en",
            file_name: t!("file-not-selected", locale = "en").into_owned(),
            texture: None,
            error: None,
            zoom: 1.0,
            auto_zoom: true,
            pending_input: None,
            convert_format: "png",
            show_metadata: false,
            metadata: None,
            image_info: None,
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
            Ok(mut image) => match utils::aloha_to_rgba8(&mut image) {
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

                    self.metadata = Some(image.metadata.clone());
                    self.image_info = Some(utils::ImageInfo {
                        width: image.width,
                        height: image.height,
                        color_type: image.color_type,
                        bit_depth: image.bit_depth,
                    });
                    self.file_name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("?")
                        .to_owned();
                    self.error = None;
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

            "jpg" => {
                let _ = aloha_convert::aloha_to_jpeg(&path, &out)
                    .map_err(|e| self.error = Some(e.to_string()));
            }

            _ => unreachable!(),
        };
    }

    fn draw_metadata_window(&mut self, ui: &mut Ui) {
        if let Some(metadata) = &self.metadata
            && self.show_metadata == true
        {
            // `.unwrap()` here is safe because the existence of `Metadata` guarantees the existance of `ImageInfo`
            let image_info = self.image_info.as_ref().unwrap();

            egui::Window::new(t!("metadata", locale = self.locale))
                .open(&mut self.show_metadata)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        egui::Grid::new("imagemetadatagrid")
                            .num_columns(2)
                            .max_col_width(ui.available_width())
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label(t!("width", locale = self.locale));
                                ui.label(format!("{}", image_info.width));
                                ui.end_row();

                                ui.label(t!("height", locale = self.locale));
                                ui.label(format!("{}", image_info.height));
                                ui.end_row();

                                ui.label(t!("color-type", locale = self.locale));
                                ui.label(format!("{:?}", image_info.color_type));
                                ui.end_row();

                                ui.label(t!("bit-depth", locale = self.locale));
                                ui.label(format!("{}", image_info.bit_depth));
                                ui.end_row();

                                if let Some(title) = &metadata.title {
                                    ui.label(t!("title", locale = self.locale));
                                    ui.label(xmp_value_to_string(title));
                                    ui.end_row();
                                }

                                if let Some(creators) = &metadata.creators {
                                    ui.label(t!("creators", locale = self.locale));
                                    ui.label(xmp_value_to_string(creators));
                                    ui.end_row();
                                }

                                if let Some(contributors) = &metadata.contributors {
                                    ui.label(t!("contributors", locale = self.locale));
                                    ui.label(xmp_value_to_string(contributors));
                                    ui.end_row();
                                }

                                if let Some(publishers) = &metadata.publishers {
                                    ui.label(t!("publishers", locale = self.locale));
                                    ui.label(xmp_value_to_string(publishers));
                                    ui.end_row();
                                }

                                if let Some(subject) = &metadata.subject {
                                    ui.label(t!("subject", locale = self.locale));
                                    ui.label(xmp_value_to_string(subject));
                                    ui.end_row();
                                }

                                if let Some(description) = &metadata.description {
                                    ui.label(t!("description", locale = self.locale));
                                    ui.label(xmp_value_to_string(description));
                                    ui.end_row();
                                }

                                if let Some(dates) = &metadata.dates {
                                    ui.label(t!("dates", locale = self.locale));
                                    ui.label(xmp_value_to_string(dates));
                                    ui.end_row();
                                }

                                if let Some(rtype) = &metadata.rtype {
                                    ui.label(t!("type", locale = self.locale));
                                    ui.label(xmp_value_to_string(rtype));
                                    ui.end_row();
                                }

                                if let Some(format) = &metadata.format {
                                    ui.label(t!("format", locale = self.locale));
                                    ui.label(xmp_value_to_string(format));
                                    ui.end_row();
                                }

                                if let Some(identifier) = &metadata.identifier {
                                    ui.label(t!("identifier", locale = self.locale));
                                    ui.label(xmp_value_to_string(identifier));
                                    ui.end_row();
                                }

                                if let Some(source) = &metadata.source {
                                    ui.label(t!("source", locale = self.locale));
                                    ui.label(xmp_value_to_string(source));
                                    ui.end_row();
                                }

                                if let Some(language) = &metadata.language {
                                    ui.label(t!("language", locale = self.locale));
                                    ui.label(xmp_value_to_string(language));
                                    ui.end_row();
                                }

                                if let Some(relations) = &metadata.relations {
                                    ui.label(t!("relations", locale = self.locale));
                                    ui.label(xmp_value_to_string(relations));
                                    ui.end_row();
                                }

                                if let Some(coverage) = &metadata.coverage {
                                    ui.label(t!("coverage", locale = self.locale));
                                    ui.label(xmp_value_to_string(coverage));
                                    ui.end_row();
                                }

                                if let Some(rights) = &metadata.rights {
                                    ui.label(t!("rights", locale = self.locale));
                                    ui.label(xmp_value_to_string(rights));
                                    ui.end_row();
                                }
                            })
                    })
                });
        }
    }

    fn draw_convert_window(&mut self, ui: &mut Ui) {
        if let Some(path) = self.pending_input.clone() {
            let modal = egui::Modal::new(egui::Id::new("convertfromalohamodal")).show(ui, |ui| {
                ui.heading(t!("select-format", locale = self.locale));

                ui.label(t!(
                    "convert-in",
                    locale = self.locale,
                    name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?")
                ));

                ui.add_space(4.0);

                egui::ComboBox::from_id_salt(egui::IdSalt::new("formatcombobox"))
                    .selected_text(self.convert_format)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.convert_format, "png", "png");
                        ui.selectable_value(&mut self.convert_format, "jpg", "jpg");
                    });
                ui.separator();

                egui::Sides::new().show(
                    ui,
                    |_| {},
                    |ui| {
                        if ui.button(t!("cancel", locale = self.locale)).clicked() {
                            ui.close();
                        }

                        if ui.button(t!("confirm", locale = self.locale)).clicked() {
                            self.convert_from(ui, &path, self.convert_format);
                            ui.close();
                        }
                    },
                );
            });

            if modal.should_close() {
                self.pending_input = None;
            }
        }
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

                if ui.button(t!("exit", locale = self.locale)).clicked() {
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

            if self.texture.is_some() {
                ui.separator();
                if ui.button(t!("metadata", locale = self.locale)).clicked() {
                    self.show_metadata = !self.show_metadata;
                }
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
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::RED, error);
            } else if let Some(texture) = &self.texture {
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
        });

        self.draw_metadata_window(ui);
        self.draw_convert_window(ui);
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
