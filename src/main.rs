extern crate lyon;

#[macro_use]
extern crate glium;

extern crate euclid;

mod drawing;

extern crate schema_parser;

use std::thread;
use std::time;


use std::fs;
use std::env;

use glium::Surface;
use glium::glutin::EventsLoop;


use lyon::extra::rust_logo::build_logo_path;
use lyon::math::*;
use lyon::tessellation::geometry_builder::{VertexConstructor, VertexBuffers, BuffersBuilder};
use lyon::tessellation::{FillTessellator, FillOptions, StrokeOptions};
use lyon::tessellation;

use lyon::tessellation::geometry_builder::SimpleBuffersBuilder;
use lyon::lyon_tessellation::basic_shapes::*;
use lyon::lyon_tessellation::geometry_builder::simple_builder;

fn main() {
    let args: Vec<String> = env::args().collect();
    if(args.len() != 2){
        println!("Please specify a .lib file.");
    } else {
        let path = &args[1];
        if let Ok(mut file) = fs::File::open(path) {
            if let Some(components) = schema_parser::parse_components(&mut file){
                run(components);
            } else {
                println!("Could not parse the library file.");
            }
        } else {
            println!("File could not be opened.");
        }
    }
}

fn run(components: Vec<schema_parser::component::Component>) {
    // Create a window
    let (w, h) = (640, 480);

    let mut eloop = EventsLoop::new();

    let window = glium::glutin::WindowBuilder::new()
        .with_dimensions(700, 700)
        .with_decorations(true)
        .with_title("Schema Renderer".to_string());

    let context = glium::glutin::ContextBuilder::new();

    let display = glium::Display::new(window, context, &eloop).unwrap();

    let mut bounding_box = (
        schema_parser::component::geometry::Point { x: 0, y: 0 },
        schema_parser::component::geometry::Point { x: 0, y: 0 }
    );


    let component = &components[0];

    let mut drawables: Vec<drawing::Drawable> = component.graphic_elements.iter()
                                                                          .filter_map(|shape| drawing::ge_to_drawable(&display, &shape))
                                                                          .collect();

    let mut running = true;

    while running {
        let mut target = display.draw();
        target.clear_color(0.8, 0.8, 0.8, 1.0);

        let (width, height) = target.get_dimensions();
        let aspect_ratio = height as f32 / width as f32;

        let fov: f32 = 3.141592 / 3.0;
        let zfar = 1024.0;
        let znear = 0.1;

        let f = 1.0 / (fov / 2.0).tan() / 2.0 / 500.0;

        let perspective = drawing::Transform2D(euclid::TypedTransform2D::create_scale(f * aspect_ratio, f));

        let uniforms  = uniform!{
            perspective: perspective
        };

        for drawable in &drawables {
            drawable.draw(&mut target, &uniforms)
        }

        target.finish().unwrap();

        eloop.poll_events(|ev| {
            match ev {
                // The window was closed
                // We break the loop and let it go out of scope, which will close it finally
                glium::glutin::Event::WindowEvent { event,.. } => {
                    match event {
                        glium::glutin::WindowEvent::Closed => { running = false; },
                        glium::glutin::WindowEvent::KeyboardInput {
                            input: glium::glutin::KeyboardInput {
                                virtual_keycode: Some(glium::glutin::VirtualKeyCode::Q),
                                modifiers: glium::glutin::ModifiersState {
                                    ctrl: true,
                                    ..
                                },
                                ..
                            },
                            ..
                        } => { running = false; }
                        _ => ()
                    }
                },
                _ => ()
            }
            let m = time::Duration::from_millis(10);
            thread::sleep(m);
        });
    }
}