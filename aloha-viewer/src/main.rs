use eframe::egui::{self, Ui};

mod utils;

struct AlohaViewer {
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
}

impl AlohaViewer {
    pub fn new(_cc: &eframe::CreationContext) -> Self {
        Self {
            texture: None,
            error: None,
        }
    }

    fn load(&mut self, ui: &mut Ui, path: &std::path::PathBuf) {
        match aloha_core::read_aloha(path.as_path()) {
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
                                path.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
                                color,
                                egui::TextureOptions::NEAREST,
                            ));
                        }
                    }
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
}

impl eframe::App for AlohaViewer {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            if ui.button("Open File").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    self.load(ui, &path);
                }
            }

            if let Some(texture) = &self.texture {
                ui.add(egui::Image::new(texture));
            }

            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::RED, error);
            }
        });
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "ALOHA Viewer",
        eframe::NativeOptions::default(),
        Box::new(|cc| Ok(Box::new(AlohaViewer::new(cc)))),
    )
}
