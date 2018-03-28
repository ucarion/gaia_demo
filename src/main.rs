#[macro_use]
extern crate error_chain;

extern crate cgmath;
extern crate fps_counter;
extern crate gaia;
extern crate gaia_assetgen;
extern crate gfx;
extern crate hsl;
extern crate piston;
extern crate piston_window;

mod camera_controller;

use camera_controller::CameraController;

use cgmath::{Angle, Matrix4, PerspectiveFov, Rad};
use fps_counter::FPSCounter;
use gaia_assetgen::Properties;
use gfx::Device;
use hsl::HSL;
use piston::window::WindowSettings;
use piston::input::Button;
use piston::input::keyboard::Key;
use piston_window::*;

use std::time::{SystemTime, UNIX_EPOCH};

error_chain!{}

enum MapMode {
    Terrain,
    All,
    Oecd,
    Income,
    Exceptional,
}

impl MapMode {
    fn should_show(&self, properties: &Properties) -> bool {
        match *self {
            MapMode::Terrain => false,
            MapMode::All | MapMode::Income => true,
            MapMode::Oecd => properties["INCOME_GRP"].as_str().unwrap() == "1. High income: OECD",
            MapMode::Exceptional => {
                properties["ADMIN"].as_str().unwrap() == "United States of America"
            }
        }
    }

    fn color(&self, properties: &Properties) -> [u8; 4] {
        match *self {
            MapMode::Terrain | MapMode::All | MapMode::Oecd => {
                let color_num = properties["MAPCOLOR13"].as_f64().unwrap() as u8;
                let (r, g, b) = HSL {
                    h: 360.0 * (color_num as f64 / 13.0),
                    s: 1.0,
                    l: 0.3,
                }.to_rgb();

                [r, g, b, 64]
            }
            MapMode::Income => match properties["INCOME_GRP"].as_str().unwrap() {
                "1. High income: OECD" => [0, 255, 0, 100],
                "2. High income: nonOECD" => [50, 200, 0, 100],
                "3. Upper middle income" => [100, 150, 0, 100],
                "4. Lower middle income" => [150, 200, 0, 100],
                "5. Low income" => [255, 0, 0, 100],
                _ => unreachable!(),
            },
            MapMode::Exceptional => {
                let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

                let secs = time.as_secs() as f64;
                let (r, g, b) = HSL {
                    h: (secs * 100.0) % 360.0,
                    s: 1.0,
                    l: 0.5,
                }.to_rgb();

                [r, g, b, 100]
            }
        }
    }
}

struct State {
    camera_controller: CameraController,
    map_mode: MapMode,
    labels_enabled: bool,
}

impl State {
    pub fn event<E>(&mut self, e: &E)
    where
        E: GenericEvent,
    {
        self.camera_controller.event(e);

        e.press(|button| match button {
            Button::Keyboard(Key::D1) => {
                self.map_mode = MapMode::Terrain;
            }
            Button::Keyboard(Key::D2) => {
                self.map_mode = MapMode::All;
            }
            Button::Keyboard(Key::D3) => {
                self.map_mode = MapMode::Oecd;
            }
            Button::Keyboard(Key::D4) => {
                self.map_mode = MapMode::Income;
            }
            Button::Keyboard(Key::D5) => {
                self.map_mode = MapMode::Exceptional;
            }
            Button::Keyboard(Key::D0) => {
                self.labels_enabled = !self.labels_enabled;
            }
            _ => {}
        });
    }

    fn desired_level(&self, camera_height: f32) -> u8 {
        if camera_height < 0.1 {
            5
        } else if camera_height < 0.2 {
            4
        } else if camera_height < 0.5 {
            3
        } else if camera_height < 0.7 {
            2
        } else {
            1
        }
    }

    fn get_mvp(&self, window: &PistonWindow) -> Matrix4<f32> {
        let draw_size = window.window.draw_size();
        let perspective = PerspectiveFov {
            fovy: Rad::full_turn() / 8.0,
            near: 0.001,
            far: 100.0,
            aspect: (draw_size.width as f32) / (draw_size.height as f32),
        };

        Matrix4::from(perspective) * self.camera_controller.view_matrix()
    }

    fn polygon_color_chooser(&self, properties: &Properties) -> Option<[u8; 4]> {
        if self.map_mode.should_show(properties) {
            Some(self.map_mode.color(properties))
        } else {
            None
        }
    }

    fn label_style_chooser<'a>(&self, properties: &'a Properties) -> Option<gaia::LabelStyle<'a>> {
        if !self.labels_enabled {
            return None;
        }

        let height = self.camera_controller.camera_height();
        let min_zoom = properties["min_zoom"].as_f64().unwrap();

        if height as f64 * min_zoom > 1.5 {
            return None;
        }

        let text = properties["NAME"].as_str().unwrap();
        let is_capital = properties["ADM0CAP"].as_f64().unwrap() == 1.0;
        let (scale, text_color) = if is_capital {
            (30.0, [1.0, 1.0, 0.0, 1.0])
        } else {
            (20.0, [1.0, 1.0, 1.0, 1.0])
        };

        Some(gaia::LabelStyle {
            text,
            scale,
            text_color,
            border_color: [0.0, 0.0, 0.0, 1.0],
            border_width: 1.0,
        })
    }
}

fn main() {
    if let Err(ref e) = run() {
        println!("error: {}", e);

        for e in e.iter().skip(1) {
            println!("caused by: {}", e);
        }

        if let Some(backtrace) = e.backtrace() {
            println!("{:?}", backtrace);
        }

        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut window: PistonWindow = WindowSettings::new("Gaia", [960, 520])
        .exit_on_esc(true)
        .opengl(OpenGL::V3_2)
        .build()
        .map_err(Error::from)?;

    let mut state = State {
        camera_controller: CameraController::new(),
        map_mode: MapMode::Terrain,
        labels_enabled: false,
    };

    let mut gaia_renderer =
        gaia::Renderer::new(window.factory.clone()).chain_err(|| "Could not create renderer")?;

    let mut fps_counter = FPSCounter::new();
    let mut fps = 0;

    let mut glyphs = Glyphs::new(
        "assets/fonts/FiraSans-Regular.ttf",
        window.factory.clone(),
        piston_window::texture::TextureSettings::new(),
    ).map_err(|_err| Error::from("glyph error"))?;

    while let Some(e) = window.next() {
        state.event(&e);

        window.draw_3d(&e, |window| {
            window
                .encoder
                .clear(&window.output_color, [0.3, 0.3, 0.3, 1.0]);
            window.encoder.clear_depth(&window.output_stencil, 1.0);
            window.encoder.clear_stencil(&window.output_stencil, 0);

            let mvp = state.get_mvp(&window);
            gaia_renderer
                .render(
                    &mut window.encoder,
                    window.output_color.clone(),
                    window.output_stencil.clone(),
                    mvp,
                    state.camera_controller.look_at(),
                    state.camera_controller.camera_height(),
                    &|properies| state.polygon_color_chooser(properies),
                    &|properies| state.label_style_chooser(properies),
                    &|camera_position| state.desired_level(camera_position),
                )
                .unwrap();

            window.device.cleanup();

            fps = fps_counter.tick();
        });

        window.draw_2d(&e, |context, graphics| {
            piston_window::rectangle(
                [1.0, 1.0, 1.0, 1.0],
                [0.0, 0.0, 200.0, 15.0],
                context.transform,
                graphics,
            );

            let camera_height = state.camera_controller.camera_height();
            text::Text::new_color([0.0, 0.0, 0.0, 1.0], 10)
                .draw(
                    &format!("FPS: {} - Camera height: {}", fps, camera_height),
                    &mut glyphs,
                    &context.draw_state,
                    context.transform.trans(10.0, 10.0),
                    graphics,
                )
                .unwrap();
        });
    }

    Ok(())
}
