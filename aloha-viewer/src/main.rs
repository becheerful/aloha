use eframe::egui::{self, Ui};

mod utils;

struct AlohaViewer {
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
    zoom: f32,
}

impl AlohaViewer {
    pub fn new(_cc: &eframe::CreationContext) -> Self {
        Self {
            texture: None,
            error: None,
            zoom: -1.0,
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
        egui::MenuBar::default().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open File").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        self.load(ui, &path);
                    }
                }

                if ui.button("Exit").clicked() {
                    let ctx = ui.ctx().clone();
                    std::thread::spawn(move || {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    });
                }
            });

            if self.texture.is_some() {
                ui.separator();
                ui.label("Scale:");
                ui.add(egui::Slider::new(&mut self.zoom, 0.1..=16.0).logarithmic(true));

                if ui.button("1:1").clicked() {
                    self.zoom = 1.0;
                }

                if ui.button("Fit").clicked() {
                    self.zoom = -1.0;
                }
            }
        });

        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(texture) = &self.texture {
                let avail = ui.available_size();
                let size = texture.size_vec2();

                let scale = if self.zoom == -1.0 {
                    (avail.x / size.x).min(avail.y / size.y)
                } else {
                    self.zoom
                };

                ui.add(egui::Image::new(texture).fit_to_exact_size(size * scale));
                self.zoom = scale;
            }

            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::RED, error);
            }
        });
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
