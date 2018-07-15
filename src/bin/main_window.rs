use std;
use std::time::{
    SystemTime,
    UNIX_EPOCH
};
use std::sync::{
    Arc,
    RwLock,
};

use env;

use gtk;
use gtk::{
    ContainerExt,
    Inhibit,
    OrientableExt,
    WidgetExt,
    BoxExt,
    GtkWindowExt,
    GLAreaExt,
    Orientation::Vertical,
};

use gdk;
use gdk::{
    EventMask,
    ModifierType,
    EventMotion,
    EventKey,
    EventButton
};

use gfx;
use gfx::traits::FactoryExt;
use gfx::Device;
use gfx::format::Formatted;

use gfx_core::memory::Typed;
use gfx_core;

use gfx_gl;

use gfx_device_gl;

use epoxy;

use relm::Widget;
use relm_attributes::widget;

use self::Msg::*;
use components::cursor_info;

use copper::drawing;
use copper::drawing::drawables;
use copper::state::schema::*;
use copper::state::event::{EventBus, Listener};
use copper::manipulation::library;
use copper::geometry::Point2D;

use copper::loading::schema_loader;
use copper::viewing::schema_viewer;
use copper::drawing::schema_drawer;

use components::cursor_info::CursorInfo;

/* Defines for gfx-rs/OGL pipeline */
pub type ColorFormat = gfx::format::Rgba8;
pub type DepthFormat = gfx::format::DepthStencil;

const CLEAR_COLOR: [f32; 4] = [0.8, 0.8, 0.8, 1.0];

const RENDER_CANVAS: [drawing::VertexRender; 6] = [
    drawing::VertexRender { position: [ -1.0, -1.0 ] },
    drawing::VertexRender { position: [  1.0, -1.0 ] },
    drawing::VertexRender { position: [  -1.0,  1.0 ] },
    drawing::VertexRender { position: [ 1.0, 1.0 ] },
    drawing::VertexRender { position: [  -1.0, 1.0 ] },
    drawing::VertexRender { position: [  1.0,  -1.0 ] }
];

pub struct Model {
    gfx_factory: Option<gfx_device_gl::Factory>,
    gfx_device: Option<gfx_device_gl::Device>,
    gfx_encoder: Option<gfx::Encoder<gfx_device_gl::Resources, gfx_device_gl::CommandBuffer> >,
    gfx_target: Option<gfx::handle::RenderTargetView<gfx_device_gl::Resources, (gfx::format::R8_G8_B8_A8, gfx::format::Unorm)>>,
    gfx_msaatarget: Option<gfx::handle::RenderTargetView<gfx_device_gl::Resources, (gfx::format::R8_G8_B8_A8, gfx::format::Unorm)>>,
    gfx_msaaview: Option<gfx::handle::ShaderResourceView<gfx_device_gl::Resources, [f32; 4]>>,
    program: Option<gfx::PipelineState<gfx_device_gl::Resources, drawing::pipe::Meta>>,
    program_render: Option<gfx::PipelineState<gfx_device_gl::Resources, drawing::pipe_render::Meta>>,
    width: i32,
    height: i32,
    ms: u64,
    nanos: u64,
    view_state: Arc<RwLock<ViewState>>,
    schema: Arc<RwLock<Schema>>,
    event_bus: EventBus,
    schema_loader: schema_loader::SchemaLoader,
    schema_viewer: schema_viewer::SchemaViewer,
    schema_drawer: Arc<Box<schema_drawer::SchemaDrawer>>,
    title: String,
}

#[derive(Msg)]
pub enum Msg {
    Quit,
    Realize,
    Unrealize,
    RenderGl(gdk::GLContext),
    Resize(i32, i32, i32),
    ButtonPressed(EventButton),
    MoveCursor(EventMotion),
    ZoomOnSchema(f64, f64),
    KeyDown(EventKey)
}

#[widget]
impl Widget for Win {
    fn init_view(&mut self) {
        self.window.add_events(
            EventMask::POINTER_MOTION_MASK.bits() as i32 |
            EventMask::SCROLL_MASK.bits() as i32 |
            EventMask::SMOOTH_SCROLL_MASK.bits() as i32 |
            EventMask::BUTTON_PRESS_MASK.bits() as i32 |
            EventMask::BUTTON_RELEASE_MASK.bits() as i32
        );
        self.gl_area.add_events(
            EventMask::POINTER_MOTION_MASK.bits() as i32 |
            EventMask::SCROLL_MASK.bits() as i32 |
            EventMask::SMOOTH_SCROLL_MASK.bits() as i32 |
            EventMask::BUTTON_PRESS_MASK.bits() as i32 |
            EventMask::BUTTON_RELEASE_MASK.bits() as i32
        );
    }
    
    // The initial model.
    fn model() -> Model {
        let event_bus = EventBus::new();

        let view_state = Arc::new(RwLock::new(ViewState::new(1, 1)));
        let schema = Arc::new(RwLock::new(Schema::new(event_bus.get_handle())));

        

        let args: Vec<String> = env::args().collect();
        if args.len() != 3 {
            println!("Please specify a .lib and a .sch file.");
            ::std::process::exit(1);
        }
        // Create a new Library from a file specified on the commandline
        let library = Arc::new(RwLock::new(library::Library::new(&args[1]).unwrap()));



        let drawer = Arc::new(Box::new(schema_drawer::SchemaDrawer::new(schema.clone(), view_state.clone(), library)));

        // Todo: Figure out how to get an Arc<Box<Listener>> out of Arc<Box<<SchemaDrawer>>
        // event_bus.get_handle().add_listener(Arc::downgrade(&drawer));

        Model {
            gfx_factory: None,
            gfx_device: None,
            gfx_encoder: None,
            gfx_target: None,
            gfx_msaatarget: None,
            gfx_msaaview: None,
            program: None,
            program_render: None,
            height: 0,
            width: 0,
            ms: 0,
            nanos: 0,
            schema_loader: schema_loader::SchemaLoader::new(schema.clone()),
            schema_viewer: schema_viewer::SchemaViewer::new(schema.clone(), view_state.clone()),
            schema_drawer: drawer,
            view_state: view_state,
            schema: schema,
            event_bus: event_bus,
            title: "Schema Renderer".to_string(),
        }
    }

    // Update the model according to the message received.
    fn update(&mut self, event: Msg) {
        //println!("{:?}", event);
        match event {
            Quit => gtk::main_quit(),
            Realize => println!("realize!"), // This will never be called because relm applies this handler after the event
            Unrealize => println!("unrealize!"),
            RenderGl(context) => self.render_gl(context),
            Resize(w,h, factor) => {
                println!("RenderArea size - w: {}, h: {}", w, h);
                {
                    let mut view_state = self.model.view_state.write().unwrap();
                    self.model.width = w;
                    self.model.height = h;
                    view_state.update_from_resize(w as u32, h as u32);
                    self.model.title = format!("Schema Renderer {:?}", Point2D::new(w as f32, h as f32));

                    view_state.update_display_scale_factor(factor);

                    // Get initial dimensions of the GlArea
                    let dim: gfx::texture::Dimensions = (
                        self.model.width as u16,
                        self.model.height as u16,
                        1,
                        gfx::texture::AaMode::Single
                    );
                    
                    // Create a initial RenderTarget with the dimensions
                    let (target, _ds_view) = gfx_device_gl::create_main_targets_raw(dim, ColorFormat::get_format().0, DepthFormat::get_format().0);
                    // Create the pipeline data struct
                    self.model.gfx_target = Some(Typed::new(target));
                }
                self.notify_view_state_changed();
            },
            ButtonPressed(event) => {
                println!("BTN DOWN {:?}", event.get_button());
                if event.get_button() == 1 {
                    let mut view_state = self.model.view_state.write().unwrap();
                    view_state.select_hovered_component();
                }
                self.notify_view_state_changed();
            },
            MoveCursor(event) => {
                {
                    let mut view_state = self.model.view_state.write().unwrap();
                    let (x, y) = event.get_position();
                    let new_state = Point2D::new(x as f32, y as f32);
                    if event.get_state().contains(ModifierType::BUTTON3_MASK) {
                        let mut movement = new_state - view_state.get_cursor();
                        movement.x /= view_state.width as f32 * view_state.get_aspect_ratio();
                        movement.y /= - view_state.height as f32;
                        view_state.center -= movement / view_state.scale * 8.0;
                        view_state.update_perspective();
                    }
                    view_state.update_cursor(new_state);
                }
                self.notify_view_state_changed();
            },
            ZoomOnSchema(_x, y) => {
                {
                    let mut view_state = self.model.view_state.write().unwrap();
                    view_state.update_from_zoom(y as f32);
                }
                self.notify_view_state_changed();
            },
            KeyDown(event) => {
                use gdk::enums::key::{ r };
                let mut schema = self.model.schema.write().unwrap();
                let view_state = self.model.view_state.read().unwrap();
                match event.get_keyval() {
                    r => {
                        view_state.hovered_component_uuid.as_ref().map(|uuid| schema.rotate_component(uuid.clone()));
                    },
                    _ => ()
                }
            }
        }
    }

    fn notify_view_state_changed(&mut self) {
        self.gl_area.queue_draw();
        self.model.schema_viewer.update_currently_hovered_component();
        let view_state = self.model.view_state.read().unwrap();
        self.cursor_info.emit(cursor_info::Msg::ViewStateChanged(view_state.clone()));
    }

    fn load_schema(&mut self) {
        /*
        * L O A D   S C H E M A
        */

        let mut schema_loader = &mut self.model.schema_loader;
        // Load library and schema file
        let args: Vec<String> = env::args().collect();

        // Load a schema form a file specified on the commandline
        schema_loader.load_from_file(args[2].clone());

        // Zoom to BB
        let mut schema = self.model.schema.write().unwrap();
        let mut view_state = self.model.view_state.write().unwrap();
        let bb = schema.get_bounding_box();
        view_state.update_from_box_pan(bb);
    }

    fn setup_render_context(&mut self) {

        // Create a new device with a getter for GL calls.
        // This can be done via libepoxy which is a layer above GL and simplifies the retrieval of the function handles
        let (device, mut factory) = gfx_device_gl::create(epoxy::get_proc_addr);
        self.model.gfx_device = Some(device);

        // Create the program
        let shader = factory.link_program(&drawables::loaders::VS_CODE, &drawables::loaders::FS_CODE).unwrap();
        let mut rasterizer = gfx::state::Rasterizer::new_fill();
        rasterizer.samples = Some(gfx::state::MultiSample);
        self.model.program = Some(factory.create_pipeline_from_program(
            &shader,
            gfx::Primitive::TriangleList,
            rasterizer,
            drawing::pipe::new()
        ).unwrap());

        let shader = factory.link_program(&drawables::loaders::VS_RENDER_CODE, &drawables::loaders::FS_RENDER_CODE).unwrap();
        let rasterizer = gfx::state::Rasterizer::new_fill();
        self.model.program_render = Some(factory.create_pipeline_from_program(
            &shader,
            gfx::Primitive::TriangleList,
            rasterizer,
            drawing::pipe_render::new()
        ).unwrap());

        // We need to select the proper FrameBuffer, as the default FrameBuffer is used by GTK itself to render the GUI
        // It then exposes a second FB which holds the RTV 
        use gfx_device_gl::FrameBuffer;
        let mut cmdbuf = factory.create_command_buffer();
        unsafe {
            let mut fbo: i32 = 0;
            std::mem::transmute::<_, extern "system" fn(gfx_gl::types::GLenum, *mut gfx_gl::types::GLint) -> ()>(
                epoxy::get_proc_addr("glGetIntegerv")
            )(gfx_gl::DRAW_FRAMEBUFFER_BINDING, &mut fbo);
            cmdbuf.display_fb = fbo as FrameBuffer;
        }
        
        // Create a new GL pipeline
        self.model.gfx_encoder = Some(gfx::Encoder::from(cmdbuf));

        // Get initial dimensions of the GlArea
        let dim: gfx::texture::Dimensions = (
            self.model.width as u16,
            self.model.height as u16,
            1,
            gfx::texture::AaMode::Single
        );
        
        // Create a initial RenderTarget with the dimensions
        let (target, _ds_view) = gfx_device_gl::create_main_targets_raw(dim, ColorFormat::get_format().0, DepthFormat::get_format().0);
        // Create the pipeline data struct
        self.model.gfx_target = Some(Typed::new(target));

        /* Create actual MSAA enabled RT */
        let (_, view_msaa, target_msaa) = create_render_target_msaa(
            &mut factory,
            self.model.width as u16,
            self.model.height as u16,
            8
        ).unwrap();

        self.model.gfx_msaatarget = Some(target_msaa);
        self.model.gfx_msaaview = Some(view_msaa);

        self.model.gfx_factory = Some(factory);
    }

    fn prepare_frame(&mut self, context: gdk::GLContext) {
        let start = SystemTime::now();
        let since_the_epoch = start.duration_since(UNIX_EPOCH)
            .expect("Time went backwards");
        let ms = since_the_epoch.as_secs() * 1000;
        let nanos = since_the_epoch.subsec_nanos() as u64;
        // println!("Time since last frame: {},{}", (ms + nanos / 1_000_000), (self.model.ms + self.model.nanos / 1_000_000));
        self.model.ms = ms;
        self.model.nanos = nanos;

        // Make the GlContext received from GTK the current one
        use gdk::GLContextExt;
        context.make_current();
    }

    fn draw_frame(&mut self) {
        let encoder = self.model.gfx_encoder.as_mut().unwrap();
        let target = self.model.gfx_target.as_mut().unwrap();
        let target_msaa = self.model.gfx_msaatarget.as_mut().unwrap();
        let view_msaa = self.model.gfx_msaaview.as_mut().unwrap();
        let factory = self.model.gfx_factory.as_mut().unwrap();
        let program = self.model.program.as_mut().unwrap();
        let program_render = self.model.program_render.as_mut().unwrap();

        let mut view_state = self.model.view_state.write().unwrap();

        // Clear the canvas
        encoder.clear(target_msaa, CLEAR_COLOR);

        // Create empty buffers
        let vbo = Vec::<drawing::Vertex>::new();
        let ibo = Vec::<u32>::new();
        let abo = Vec::<drawing::Attributes>::new();
        let mut buffers = drawing::Buffers {
            vbo: vbo,
            ibo: ibo,
            abo: abo,
        };

        // Fill buffers
        self.model.schema_drawer.draw(&mut buffers);
        // view_state.selected_component_uuid.map(|v| {
        //     visual_helpers::draw_selection_indicator(&mut buffers, v);
        // });

        let (vbo, ibo) = factory.create_vertex_buffer_with_slice(
            &buffers.vbo[..],
            &buffers.ibo[..]
        );

        // Create per drawable attributes buffer
        let attributes = factory.create_constant_buffer(800);

        // Create bundle
        let buf = factory.create_constant_buffer(1);
        let bundle = gfx::pso::bundle::Bundle::new(
            ibo,
            program.clone(),
            drawing::pipe::Data {
                vbuf: vbo,
                globals: buf,
                out: target_msaa.clone(),
                attributes: attributes,
            }
        );
        let perspective = view_state.current_perspective.clone();
        let globals = drawing::Globals {
            perspective: perspective.into()
        };

        // Add bundle to the pipeline
        encoder.update_constant_buffer(&bundle.data.globals, &globals);
        encoder.update_buffer(&bundle.data.attributes, &buffers.abo, 0).unwrap();
        bundle.encode(encoder);

        // TODO: Put to another location as this never changes and doesn't need to be done each frame
        let (vertex_buffer, slice) = factory.create_vertex_buffer_with_slice(&RENDER_CANVAS, ());

        // TODO: Put to another location as this never changes and doesn't need to be done each frame
        use gfx::Factory;
        let sampler = factory.create_sampler(gfx::texture::SamplerInfo::new(
            gfx::texture::FilterMethod::Trilinear,
            gfx::texture::WrapMode::Tile,
        ));

        // Finalize image with render to final target
        let bundle = gfx::pso::bundle::Bundle::new(
            slice,
            program_render.clone(),
            drawing::pipe_render::Data {
                vbuf: vertex_buffer,
                texture: (view_msaa.clone(), sampler), 
                out: target.clone()
            }
        );

        bundle.encode(encoder);
    }

    fn finalize_frame(&mut self) {
        let encoder = self.model.gfx_encoder.as_mut().unwrap();
        let device = self.model.gfx_device.as_mut().unwrap();
        encoder.flush(device);
        // TODO: swap buffers
        device.cleanup();
        let start = SystemTime::now();
        let since_the_epoch = start.duration_since(UNIX_EPOCH)
            .expect("Time went backwards");
        let end = since_the_epoch.as_secs() * 1_000_000 + since_the_epoch.subsec_nanos() as u64 / 1000;
        let start = self.model.ms * 1000 + self.model.nanos / 1000;
        println!("Frametime in us: {}", end - start);
    }

    fn render_gl(&mut self, context: gdk::GLContext) {
        self.prepare_frame(context);

        // Init GL machinery in the first draw as we can't catch the realize event
        if self.model.gfx_factory.is_none() {
            self.load_schema();
            self.setup_render_context();
        }

        self.draw_frame();
        self.finalize_frame();
    }

    view! {
        #[name="window"]
        gtk::Window {
            can_focus: false,
            border_width: 1,
            property_default_width: 1800,
            property_default_height: 1000,
            realize => Realize,
            title: &self.model.title,

            child: {
                expand: true,
                fill: true,
            },

            #[name="main_box"]
            gtk::Box {
                orientation: Vertical,
                can_focus: false,
                spacing: 6,
                realize => Realize,

                #[name="gl_area"]
                gtk::GLArea {
                    can_focus: false,
                    hexpand: true,
                    vexpand: true,
                    realize => Realize,
                    unrealize => Unrealize,
                    resize(area, width, height) => Resize(width, height, area.get_scale_factor()),
                    render(area, context) => ({
                        let rgl = RenderGl(context.clone());
                        area.queue_render();
                        rgl
                    }, Inhibit(true)),
                    button_press_event(_, event) => ({
                        ButtonPressed(event.clone())
                    }, Inhibit(false)),
                    motion_notify_event(_, event) => (MoveCursor(event.clone()), Inhibit(false)),
                    scroll_event(_, event) => (ZoomOnSchema(
                        event.get_delta().0,
                        event.get_delta().1,
                    ), Inhibit(false)),
                },
                #[name="cursor_info"]
                CursorInfo {

                },
            },
            key_press_event(_, event) => (KeyDown(event.clone()), Inhibit(false)),
            delete_event(_, _) => (Quit, Inhibit(false)),
        }
    }
}

fn create_render_target_msaa<T: gfx_core::format::RenderFormat + gfx_core::format::TextureFormat, R: gfx_core::Resources, F> (
    factory: &mut F,
    width: gfx_core::texture::Size,
    height: gfx_core::texture::Size,
    msaa: u8
) -> Result<
    (
        gfx_core::handle::Texture<R, T::Surface>,
        gfx_core::handle::ShaderResourceView<R, T::View>,
        gfx_core::handle::RenderTargetView<R, T>
    ),
    gfx_core::factory::CombinedError
> where F: gfx_core::factory::Factory<R> {
    let kind = gfx_core::texture::Kind::D2(width, height, gfx_core::texture::AaMode::Multi(msaa));
    let levels = 1;
    let cty = <T::Channel as gfx_core::format::ChannelTyped>::get_channel_type();
    let tex = try!(factory.create_texture(kind, levels, gfx_core::memory::Bind::RENDER_TARGET | gfx_core::memory::Bind::SHADER_RESOURCE, gfx_core::memory::Usage::Data, Some(cty)));
    let view = try!(factory.view_texture_as_shader_resource::<T>(&tex, (levels, levels), gfx::format::Swizzle::new()));
    let target = try!(factory.view_texture_as_render_target(&tex, 0, None));
    Ok((tex, view, target))
}