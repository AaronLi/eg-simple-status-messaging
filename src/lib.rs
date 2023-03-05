use std::error::Error;
use std::fmt::Display;
use std::ops::DerefMut;
use std::os;
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{JoinHandle, sleep, spawn};
use std::time::{Duration, Instant};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::PixelColor;
use embedded_graphics::prelude::Point;
use u8g2_fonts::{FontRenderer, U8g2TextStyle};
use u8g2_fonts::types::{FontColor, VerticalPosition};
use ws2812_esp32_rmt_driver::lib_embedded_graphics::{LedPixelMatrix, Ws2812DrawTarget};
use ws2812_esp32_rmt_driver::{Ws2812Esp32RmtDriver, Ws2812Esp32RmtDriverError};
use ws2812_esp32_rmt_driver::driver::color::{LedPixelColor, LedPixelColorRgbw32};

#[derive(Debug)]
pub struct LedPrinter<C, E, Target> where Target: DrawTarget<Color=C, Error=E>, C: PixelColor, E: Error {
    draw_target: Arc<RwLock<Target>>,
    scroll_spp_ms: u16,
    display_task: Option<JoinHandle<()>>,
    display_task_controller: Arc<Mutex<bool>>
}

#[derive(Debug, Copy, Clone)]
enum Direction {
    Left,
    Right
}

impl<C, E, Target> LedPrinter<C, E, Target> where Target: DrawTarget<Color=C, Error=E> + Send + Sync + 'static, C: PixelColor + Send + 'static, E: Error {
    pub fn new(target: Arc<RwLock<Target>>, scroll_ms_per_pixel: u16) -> Result<Self, E> {
        Ok(LedPrinter{
            draw_target: target,
            scroll_spp_ms: scroll_ms_per_pixel,
            display_task: None,
            display_task_controller: Arc::new(Mutex::new(false))
        })
    }

    fn display_task(target: Arc<RwLock<impl DrawTarget<Color=C, Error=E>>>, spp: u16, task_controller: Arc<Mutex<bool>>, to_display: String, black: C, color: C){
        let renderer = FontRenderer::new::<u8g2_fonts::fonts::u8g2_font_standardized3x5_tr>();
        let width = renderer.get_rendered_dimensions(to_display.as_str(), Point::zero(), VerticalPosition::Top).unwrap().bounding_box.unwrap().size.width as i32;
        let mut x_pos = 0;
        let mut direction = Direction::Right;
        let step_period = 10;
        let mut previous_update = 0;
        let spp = spp as u64;
        let mut running = true;
        let mut target_locked = target.write().unwrap();
        target_locked.clear(black);
        renderer.render(&*to_display, Point::new(-x_pos, -1), VerticalPosition::Top, FontColor::Transparent(color), target_locked.deref_mut()).unwrap();
        drop(target_locked);
        while running {
            if previous_update * step_period > spp {
                target_locked = target.write().unwrap();
                target_locked.clear(black);
                renderer.render(&*to_display, Point::new(-x_pos, -1), VerticalPosition::Top, FontColor::Transparent(color), target_locked.deref_mut()).unwrap();
                drop(target_locked);
                match direction {
                    Direction::Left => {
                        if x_pos == 0 {
                            direction = Direction::Right;
                        } else {
                            x_pos -= 1;
                        }
                    },
                    Direction::Right => {
                        if x_pos == width {
                            direction = Direction::Left;
                        } else {
                            x_pos += 1;
                        }
                    }
                }
                previous_update = 0;
            }else {
                previous_update += 1;
            }
            sleep(Duration::from_millis(step_period));
            let controller = task_controller.lock().unwrap();
            running = *controller;
            println!("{x_pos} {direction:?}");
        }
    }

    pub fn display(&mut self, text: &str, color: C, black: C) {
        if let Some(handle) = self.display_task.take() {
            let mut task_controller = self.display_task_controller.lock().expect("Failed to lock");
            *task_controller = false;
            drop(task_controller);
            println!("joining");
            handle.join().unwrap();
        }
        let mut task_controller = self.display_task_controller.lock().expect("Failed to lock");

        *task_controller = true;

        let draw_target = Arc::clone(&self.draw_target);
        let task_controller = Arc::clone(&self.display_task_controller);
        let text = text.to_string();
        let spp = self.scroll_spp_ms;
        let _ = self.display_task.insert(spawn(move ||{Self::display_task(draw_target, spp, task_controller, text, black.clone(), color.clone())}));
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Deref;
    use embedded_graphics::pixelcolor::Rgb888;
    use embedded_graphics::prelude::Size;
    use embedded_graphics_simulator::{OutputSettings, OutputSettingsBuilder, SimulatorEvent, Window};
    use embedded_graphics_simulator::sdl2::Keycode;
    use ws2812_esp32_rmt_driver::driver::color::LedPixelColorImpl;
    use ws2812_esp32_rmt_driver::lib_embedded_graphics::{LedPixelDrawTarget, LedPixelShape};
    use super::*;

    #[test]
    fn it_works() {
        let screen = Arc::new(RwLock::new(embedded_graphics_simulator::SimulatorDisplay::<Rgb888>::new(Size::new(5, 5))));
        let mut printer = LedPrinter::new(Arc::clone(&screen), 75).unwrap();
        let output_settings = OutputSettingsBuilder::new().scale(20).build();

        let mut window = Window::new("Preview", &output_settings);

        printer.display("Hello, World!", Rgb888::new(255, 255, 255), Rgb888::new(0, 0, 0));
        let mut running = true;
        while running {
            window.update(screen.read().unwrap().deref());
            for e in window.events() {
                match e {
                    SimulatorEvent::Quit => running = false,
                    SimulatorEvent::KeyDown {
                        keycode, keymod, repeat
                    } => {
                        let to_show = match keycode {
                            Keycode::W => "REEEE",
                            Keycode::A => "WOOOOW",
                            _ => "Hello, World!"
                        };
                        println!("Attempting to display {to_show}");
                        printer.display(to_show, Rgb888::new(255, 255, 255), Rgb888::new(0, 0, 0))
                    }
                    _ => {}
                }
            }
            sleep(Duration::from_millis(16));
        }
    }
}
