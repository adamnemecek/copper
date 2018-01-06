use std::ops;
use std::cell::RefCell;
use std::rc::Rc;


use gfx;
use gfx_device_gl;
use gfx_glyph;
use euclid;
use lyon;

use lyon::tessellation;


use schema_parser::component;
use schema_parser::component::geometry;
use schema_parser::component::geometry::SchemaSpace;
use resource_manager;


pub struct ScreenSpace;

pub type ColorFormat = gfx::format::Rgba8;
pub type DepthFormat = gfx::format::DepthStencil;
type Resources = gfx_device_gl::Resources;


gfx_defines!{
    vertex Vertex {
        position: [f32; 2] = "position",
    }

    constant Locals {
        color: [f32; 4] = "color",
        perspective: [[f32; 4]; 4] = "perspective",
    }

    pipeline pipe {
        vbuf: gfx::VertexBuffer<Vertex> = (),
        locals: gfx::ConstantBuffer<Locals> = "Locals",
        out: gfx::RenderTarget<ColorFormat> = "Target0",
    }
}

/* * * * * * * * * * * * * * * * * * * *
 *
 * Vertex Ops
 *
 * * * * * * * * * * * * * * * * * * * */

impl Vertex {
    pub fn x(&self) -> f32 { self.position[0] }
    pub fn y(&self) -> f32 { self.position[1] }
    // pub fn new(x: f32, y: f32) -> Vertex { Vertex { position: [x, y] } }
}

pub struct VertexCtor;
impl lyon::lyon_tessellation::VertexConstructor<tessellation::FillVertex, Vertex> for VertexCtor {
    fn new_vertex(&mut self, vertex: tessellation::FillVertex) -> Vertex {
        assert!(!vertex.position.x.is_nan());
        assert!(!vertex.position.y.is_nan());
        
        Vertex {
            position: vertex.position.to_array(),
        }
    }
}
impl lyon::lyon_tessellation::VertexConstructor<tessellation::StrokeVertex, Vertex> for VertexCtor {
    fn new_vertex(&mut self, vertex: tessellation::StrokeVertex) -> Vertex {
        assert!(!vertex.position.x.is_nan());
        assert!(!vertex.position.y.is_nan());
        Vertex {
            position: vertex.position.to_array(),
        }
    }
}

impl ops::Add<Vertex> for Vertex {
    type Output = Vertex;

    fn add(self, _rhs: Vertex) -> Vertex {
        Vertex {
            position: [
                self.x() + _rhs.x(),
                self.y() + _rhs.y()
            ]
        }
    }
}

impl ops::Sub<Vertex> for Vertex {
    type Output = Vertex;

    fn sub(self, _rhs: Vertex) -> Vertex {
        Vertex {
            position: [
                self.x() - _rhs.x(),
                self.y() - _rhs.y()
            ]
        }
    }
}

// impl glium::uniforms::AsUniformValue for Vertex {
//     fn as_uniform_value(&self) -> glium::uniforms::UniformValue {
//         glium::uniforms::UniformValue::Vec2(self.position)
//     }
// }

/* * * * * * * * * * * * * * * * * * * *
 *
 * Color Ops
 *
 * * * * * * * * * * * * * * * * * * * */

#[derive(Copy, Clone)]
pub struct Color {
    pub color: [f32; 4]
}

impl Color {
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Color { Color { color: [r, g, b, a] } }
}

/* * * * * * * * * * * * * * * * * * * *
 *
 * Transform Ops
 *
 * * * * * * * * * * * * * * * * * * * */

pub type Transform3D = euclid::TypedTransform3D<f32, SchemaSpace, ScreenSpace>;

pub struct DrawableObject<R: gfx::Resources> {
    bundle: gfx::pso::bundle::Bundle<R, pipe::Data<R>>,
    color: Color
}

impl DrawableObject<Resources> {
    pub fn new(bundle: gfx::pso::bundle::Bundle<Resources, pipe::Data<Resources>>, color: Color) -> Self {
        DrawableObject {
            bundle: bundle,
            color: color
        }
    }
}

impl Drawable for DrawableObject<Resources> {
    fn draw(&self, resource_manager: Rc<RefCell<resource_manager::ResourceManager>>, perspective: Transform3D){
        let locals = Locals {
            perspective: perspective.to_row_arrays(),
            color: self.color.color,
        };
        resource_manager.borrow_mut().encoder.update_constant_buffer(&self.bundle.data.locals, &locals);

        self.bundle.encode(&mut resource_manager.borrow_mut().encoder);
    }
}

pub struct GroupDrawable {
    drawables: Vec<Box<Drawable>>
}

impl GroupDrawable {
    pub fn default() -> Self {
        GroupDrawable {
            drawables: Vec::new()
        }
    }

    pub fn add<T: 'static + Drawable>(&mut self, drawable: T) {
        self.drawables.push(Box::new(drawable));
    }
}

impl Drawable for GroupDrawable {
    fn draw(&self, resource_manager: Rc<RefCell<resource_manager::ResourceManager>>, perspective: Transform3D) {
        for drawable in &self.drawables {
            drawable.draw(resource_manager.clone(), perspective.clone());
        }
    }
}

pub struct TextDrawable {
    pub position: geometry::Point,
    pub content: String,
    pub dimension: f32,
    pub orientation: geometry::TextOrientation,
    pub hjustify: component::Justify,
    pub vjustify: component::Justify
}

impl Drawable for TextDrawable {
    fn draw(&self, resource_manager: Rc<RefCell<resource_manager::ResourceManager>>, perspective: Transform3D) {
        let (w, h, _z, _aamode) = resource_manager.borrow().target.clone().get_dimensions();

        // Transform Schema coords to Screen coords
        let position_schema = euclid::TypedPoint3D::<f32, SchemaSpace>::new(self.position.x as f32, self.position.y as f32, 0.0);
        let mut position_screen = perspective.transform_point3d(&position_schema);
        position_screen.x = (position_screen.x + 1.0) / 2.0 *  (w as f32);
        position_screen.y = (position_screen.y - 1.0) / 2.0 * -(h as f32);

        let px_per_schema = match self.orientation {
            geometry::TextOrientation::Horizontal => (w as f32) / (2.0 / perspective.m11),
            geometry::TextOrientation::Vertical => (h as f32) / (2.0 / perspective.m22)
        };

        let font = {
            let rm = resource_manager.borrow_mut();
            rm.get_font(resource_manager::FontKey::new("test_data/Inconsolata-Regular.ttf"))
        };

        let mut layout = gfx_glyph::Layout::default();

        match self.hjustify {
            component::Justify::Left => { layout = layout.h_align(gfx_glyph::HorizontalAlign::Left); },
            component::Justify::Right => { layout = layout.h_align(gfx_glyph::HorizontalAlign::Right); },
            component::Justify::Center => { layout = layout.h_align(gfx_glyph::HorizontalAlign::Center); },
            _ => {}
        }

        // TODO: Add Center & Bottom (needs pull request to gfx_glyph)
        match self.vjustify {
            component::Justify::Top => { layout = layout.v_align(gfx_glyph::VerticalAlign::Top); },
            component::Justify::Bottom => { layout = layout.v_align(gfx_glyph::VerticalAlign::Top); },
            component::Justify::Center => { layout = layout.v_align(gfx_glyph::VerticalAlign::Top); },
            _ => {}
        }

        // let transform = {
        //     let aspect = h as f32 / w as f32;
        //     let zoom = 1.0;
        //     let origin = (0.0, 0.0); // top-corner: `let origin = (1.0 * aspect, -1.0);`
        //     let projection = euclid::TypedTransform3D::<f32, SchemaSpace, SchemaSpace>::ortho(
        //         origin.0 - zoom * aspect,
        //         origin.0 + zoom * aspect,
        //         origin.1 - zoom,
        //         origin.1 + zoom,
        //         1.0,
        //         -1.0,
        //     );
        //     let mut m = euclid::TypedTransform3D::<f32, SchemaSpace, SchemaSpace>::row_major(
        //         0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0
        //     );
        //     projection.post_mul(&m).post_mul(&projection.inverse().unwrap())
        // };

        let transform = euclid::TypedTransform3D::<f32, SchemaSpace, SchemaSpace>::identity();

        let section = gfx_glyph::Section {
            text: &self.content,
            screen_position: (
                position_screen.x as f32,
                position_screen.y as f32
            ),
            scale: gfx_glyph::Scale::uniform(self.dimension * px_per_schema),
            layout: layout,
            ..gfx_glyph::Section::default()
        };

        let mut f = font.borrow_mut();
        f.queue(section);
        let t = resource_manager.borrow().target.clone();
        let r = resource_manager.borrow().depth_stencil.clone();
        f.draw_queued_with_transform(transform.to_row_arrays(), &mut resource_manager.borrow_mut().encoder, &t, &r).unwrap();
    }
}

pub trait Drawable {
    fn draw(&self, resource_manager: Rc<RefCell<resource_manager::ResourceManager>>, perspective: Transform3D);
}

pub struct ViewState {
    pub current_perspective: Transform3D,
    pub width: isize,
    pub height: isize,
    pub scale: f32,
    center: euclid::TypedPoint3D<f32, SchemaSpace>,
    pub cursor: euclid::TypedPoint3D<f32, ScreenSpace>
}

impl ViewState {
    pub fn new(w: u32, h: u32) -> ViewState {
        let mut vs = ViewState {
            current_perspective: Transform3D::identity().into(),
            width: w as isize,
            height: h as isize,
            scale: 1.0 / 6000.0,
            center: euclid::TypedPoint3D::origin(),
            cursor: euclid::TypedPoint3D::origin()
        };
        vs.update_perspective();
        vs
    }

    pub fn update_from_resize(&mut self, width: u32, height: u32) {
        self.width = width as isize;
        self.height = height as isize;
        self.update_perspective();
    }

    pub fn update_from_zoom(&mut self, delta: f32) {
        self.scale += delta / 10000.0;
        if self.scale < 1.0 / 60000.0 {
            self.scale = 1.0 / 60000.0;
        }
        if self.scale > 0.3 {
            self.scale = 0.3;
        }
        self.update_perspective();
    }

    pub fn update_from_box_pan(&mut self, &(ref min, ref max): &(component::geometry::Point, component::geometry::Point)) {
        let m = (max.x - min.x).max(max.y - min.y);
        if m > 0.0 {
            self.scale = 2.45 / m;
            let w = max.x + min.x;
            let h = max.y + min.y;
            self.center = euclid::TypedPoint2D::new(
                -w / 2.0,
                -h / 2.0
            ).to_3d();
            self.update_perspective();
        }
    }

    pub fn update_perspective(&mut self) {
        let aspect_ratio = (self.height as f32) / (self.width as f32);

        self.current_perspective = euclid::TypedTransform3D::<f32, SchemaSpace, ScreenSpace>::create_scale(self.scale * aspect_ratio, self.scale, 1.0)
                                                            .pre_translate(self.center - euclid::TypedPoint3D::origin());
    }

    pub fn screen_space_to_pixels(&self, distance: f32) -> usize {
        (self.scale * distance / self.height as f32) as usize
    }
}