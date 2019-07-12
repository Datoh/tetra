use std::cell::RefCell;
use std::mem;
use std::rc::Rc;

use glow::Context as GlowContext;

use crate::error::{Result, TetraError};
use crate::glm::{self, Mat4};
use crate::graphics::{
    BufferUsage, Canvas, FilterMode, FrontFace, IndexBuffer, Shader, Texture, UniformValue,
    VertexBuffer,
};

type GlContext = glow::native::Context;

type BufferId = <GlContext as GlowContext>::Buffer;
type ProgramId = <GlContext as GlowContext>::Program;
type TextureId = <GlContext as GlowContext>::Texture;
type FramebufferId = <GlContext as GlowContext>::Framebuffer;
type VertexArrayId = <GlContext as GlowContext>::VertexArray;

pub struct GLDevice {
    gl: Rc<GlContext>,

    current_vertex_buffer: Option<BufferId>,
    current_index_buffer: Option<BufferId>,
    current_program: Option<ProgramId>,
    current_texture: Option<TextureId>,
    current_framebuffer: Option<FramebufferId>,
    current_vertex_array: Option<VertexArrayId>,

    // TODO: I kinda don't like this being here, should probably be on the graphics context.
    default_filter_mode: FilterMode,
}

impl GLDevice {
    pub(crate) fn new(gl: GlContext) -> Result<GLDevice> {
        unsafe {
            gl.enable(glow::CULL_FACE);
            gl.enable(glow::BLEND);

            // This default might want to change if we introduce
            // custom blending modes.
            gl.blend_func_separate(
                glow::SRC_ALPHA,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
            );

            // This is only needed for Core GL - if we wanted to be uber compatible, we'd
            // turn it off on older versions.
            let current_vertex_array = gl.create_vertex_array().map_err(TetraError::OpenGl)?;
            gl.bind_vertex_array(Some(current_vertex_array));

            println!("OpenGL Device: {}", gl.get_parameter_string(glow::RENDERER));
            println!("OpenGL Driver: {}", gl.get_parameter_string(glow::VERSION));
            println!("OpenGL Vendor: {}", gl.get_parameter_string(glow::VENDOR));

            // TODO: Find a nice way of exposing this via the platform layer
            // println!("Swap Interval: {:?}", video.gl_get_swap_interval());

            Ok(GLDevice {
                gl: Rc::new(gl),

                current_vertex_buffer: None,
                current_index_buffer: None,
                current_program: None,
                current_texture: None,
                current_framebuffer: None,
                current_vertex_array: Some(current_vertex_array),

                default_filter_mode: FilterMode::Nearest,
            })
        }
    }

    pub(crate) fn clear(&mut self, r: f32, g: f32, b: f32, a: f32) {
        unsafe {
            self.gl.clear_color(r, g, b, a);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }
    }

    pub(crate) fn front_face(&mut self, front_face: FrontFace) {
        unsafe {
            self.gl.front_face(front_face.into());
        }
    }

    pub(crate) fn viewport(&mut self, x: i32, y: i32, width: i32, height: i32) {
        unsafe {
            self.gl.viewport(x, y, width, height);
        }
    }

    pub(crate) fn draw_elements(&mut self, index_buffer: &IndexBuffer, count: i32) {
        unsafe {
            self.bind_index_buffer(Some(index_buffer));

            self.gl
                .draw_elements(glow::TRIANGLES, count, glow::UNSIGNED_INT, 0);
        }
    }

    pub(crate) fn create_vertex_buffer(
        &mut self,
        count: usize,
        stride: usize,
        usage: BufferUsage,
    ) -> Result<VertexBuffer> {
        unsafe {
            let id = self.gl.create_buffer().map_err(TetraError::OpenGl)?;

            let handle = GLVertexBuffer {
                gl: Rc::clone(&self.gl),
                id,
                count,
                stride,
            };

            let buffer = VertexBuffer {
                handle: Rc::new(handle),
            };

            self.bind_vertex_buffer(Some(&buffer));

            self.gl.buffer_data_size(
                glow::ARRAY_BUFFER,
                (count * mem::size_of::<f32>()) as i32,
                usage.into(),
            );

            Ok(buffer)
        }
    }

    pub(crate) fn bind_vertex_buffer(&mut self, buffer: Option<&VertexBuffer>) {
        unsafe {
            let id = buffer.map(|x| x.handle.id);

            if self.current_vertex_buffer != id {
                self.gl.bind_buffer(glow::ARRAY_BUFFER, id);
                self.current_vertex_buffer = id;
            }
        }
    }

    pub(crate) fn set_vertex_buffer_attribute(
        &mut self,
        buffer: &VertexBuffer,
        index: u32,
        size: i32,
        offset: usize,
    ) {
        // TODO: This feels a bit unergonomic...

        unsafe {
            self.bind_vertex_buffer(Some(buffer));

            self.gl.vertex_attrib_pointer_f32(
                index,
                size,
                glow::FLOAT,
                false,
                (buffer.handle.stride * mem::size_of::<f32>()) as i32,
                (offset * mem::size_of::<f32>()) as i32,
            );

            self.gl.enable_vertex_attrib_array(index);
        }
    }

    pub(crate) fn set_vertex_buffer_data(
        &mut self,
        buffer: &VertexBuffer,
        data: &[f32],
        offset: usize,
    ) {
        unsafe {
            self.bind_vertex_buffer(Some(buffer));

            // TODO: What if we want to discard what's already there?
            // TODO: Is this cast safe?

            let byte_len = std::mem::size_of_val(data) / std::mem::size_of::<u8>();
            let byte_slice = std::slice::from_raw_parts(data.as_ptr() as *const u8, byte_len);

            self.gl.buffer_sub_data_u8_slice(
                glow::ARRAY_BUFFER,
                (offset * mem::size_of::<f32>()) as i32,
                byte_slice,
            );
        }
    }

    pub(crate) fn create_index_buffer(
        &mut self,
        count: usize,
        usage: BufferUsage,
    ) -> Result<IndexBuffer> {
        unsafe {
            let id = self.gl.create_buffer().map_err(TetraError::OpenGl)?;

            let handle = GLIndexBuffer {
                gl: Rc::clone(&self.gl),
                id,
                count,
            };

            let buffer = IndexBuffer {
                handle: Rc::new(handle),
            };

            self.bind_index_buffer(Some(&buffer));

            self.gl.buffer_data_size(
                glow::ELEMENT_ARRAY_BUFFER,
                (count * mem::size_of::<u32>()) as i32,
                usage.into(),
            );

            Ok(buffer)
        }
    }

    pub(crate) fn bind_index_buffer(&mut self, buffer: Option<&IndexBuffer>) {
        unsafe {
            let id = buffer.map(|x| x.handle.id);

            if self.current_index_buffer != id {
                self.gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, id);
                self.current_index_buffer = id;
            }
        }
    }

    pub(crate) fn set_index_buffer_data(
        &mut self,
        buffer: &IndexBuffer,
        data: &[u32],
        offset: usize,
    ) {
        unsafe {
            self.bind_index_buffer(Some(buffer));

            // TODO: What if we want to discard what's already there?
            // TODO: Is this cast safe?

            let byte_len = std::mem::size_of_val(data) / std::mem::size_of::<u8>();
            let byte_slice = std::slice::from_raw_parts(data.as_ptr() as *const u8, byte_len);

            self.gl.buffer_sub_data_u8_slice(
                glow::ELEMENT_ARRAY_BUFFER,
                (offset * mem::size_of::<u32>()) as i32,
                byte_slice,
            );
        }
    }

    pub(crate) fn create_texture(
        &mut self,
        width: i32,
        height: i32,
        data: &[u8],
    ) -> Result<Texture> {
        let expected = (width * height * 4) as usize;
        let actual = data.len();

        if expected > actual {
            return Err(TetraError::NotEnoughData { expected, actual });
        }

        let texture = self.create_texture_empty(width, height)?;

        self.set_texture_data(&texture, &data, 0, 0, width, height);

        Ok(texture)
    }

    pub(crate) fn create_texture_empty(&mut self, width: i32, height: i32) -> Result<Texture> {
        unsafe {
            let id = self.gl.create_texture().map_err(TetraError::OpenGl)?;

            let handle = GLTexture {
                gl: Rc::clone(&self.gl),

                id,
                width,
                height,
                filter_mode: self.default_filter_mode,
            };

            let texture = Texture {
                handle: Rc::new(RefCell::new(handle)),
            };

            self.bind_texture(Some(&texture));

            self.gl
                .tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);

            self.gl
                .tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);

            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                self.default_filter_mode.into(),
            );

            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                self.default_filter_mode.into(),
            );

            // TODO: I don't think we need mipmaps?

            self.gl
                .tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_BASE_LEVEL, 0);

            self.gl
                .tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAX_LEVEL, 0);

            self.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32, // love 2 deal with legacy apis
                width,
                height,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                None,
            );

            Ok(texture)
        }
    }

    pub(crate) fn bind_texture(&mut self, texture: Option<&Texture>) {
        unsafe {
            let id = texture.map(|x| x.handle.borrow().id);

            if self.current_texture != id {
                self.gl.active_texture(glow::TEXTURE0);
                self.gl.bind_texture(glow::TEXTURE_2D, id);
                self.current_texture = id;
            }
        }
    }

    pub(crate) fn set_texture_data(
        &mut self,
        texture: &Texture,
        data: &[u8],
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) {
        unsafe {
            self.bind_texture(Some(texture));

            self.gl.tex_sub_image_2d_u8_slice(
                glow::TEXTURE_2D,
                0,
                x,
                y,
                width,
                height,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                Some(data),
            )
        }
    }

    pub(crate) fn set_texture_filter_mode(&mut self, texture: &Texture, filter_mode: FilterMode) {
        self.bind_texture(Some(texture));

        unsafe {
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                filter_mode.into(),
            );

            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                filter_mode.into(),
            );
        }

        texture.handle.borrow_mut().filter_mode = filter_mode;
    }

    pub(crate) fn create_shader(
        &mut self,
        vertex_shader: &str,
        fragment_shader: &str,
    ) -> Result<Shader> {
        unsafe {
            let program_id = self.gl.create_program().map_err(TetraError::OpenGl)?;

            // TODO: IDK if this should be applied to *all* shaders...
            self.gl.bind_attrib_location(program_id, 0, "a_position");
            self.gl.bind_attrib_location(program_id, 1, "a_uv");
            self.gl.bind_attrib_location(program_id, 2, "a_color");
            self.gl.bind_frag_data_location(program_id, 0, "o_color");

            let vertex_id = self
                .gl
                .create_shader(glow::VERTEX_SHADER)
                .map_err(TetraError::OpenGl)?;

            self.gl.shader_source(vertex_id, vertex_shader);
            self.gl.compile_shader(vertex_id);
            self.gl.attach_shader(program_id, vertex_id);

            if !self.gl.get_shader_compile_status(vertex_id) {
                return Err(TetraError::OpenGl(self.gl.get_shader_info_log(vertex_id)));
            }

            let fragment_id = self
                .gl
                .create_shader(glow::FRAGMENT_SHADER)
                .map_err(TetraError::OpenGl)?;

            self.gl.shader_source(fragment_id, fragment_shader);
            self.gl.compile_shader(fragment_id);
            self.gl.attach_shader(program_id, fragment_id);

            if !self.gl.get_shader_compile_status(vertex_id) {
                return Err(TetraError::OpenGl(self.gl.get_shader_info_log(fragment_id)));
            }

            self.gl.link_program(program_id);

            if !self.gl.get_program_link_status(program_id) {
                return Err(TetraError::OpenGl(self.gl.get_program_info_log(program_id)));
            }

            self.gl.delete_shader(vertex_id);
            self.gl.delete_shader(fragment_id);

            let handle = GLProgram {
                gl: Rc::clone(&self.gl),
                id: program_id,
            };

            let shader = Shader {
                handle: Rc::new(handle),
            };

            self.set_uniform(&shader, "u_texture", 0);

            Ok(shader)
        }
    }

    pub(crate) fn bind_shader(&mut self, shader: Option<&Shader>) {
        unsafe {
            let id = shader.map(|x| x.handle.id);

            if self.current_program != id {
                self.gl.use_program(id);
                self.current_program = id;
            }
        }
    }

    pub(crate) fn set_uniform<T>(&mut self, shader: &Shader, name: &str, value: T)
    where
        T: UniformValue,
    {
        unsafe {
            self.bind_shader(Some(shader));

            let location = self.gl.get_uniform_location(shader.handle.id, name);
            value.set_uniform(shader, location);
        }
    }

    pub(crate) fn create_canvas(
        &mut self,
        width: i32,
        height: i32,
        rebind_previous: bool,
    ) -> Result<Canvas> {
        unsafe {
            let texture = self.create_texture_empty(width, height)?;

            let id = self.gl.create_framebuffer().map_err(TetraError::OpenGl)?;

            let handle = GLFramebuffer {
                gl: Rc::clone(&self.gl),
                id,
            };

            let canvas = Canvas {
                texture,
                framebuffer: Rc::new(handle),
                projection: glm::ortho(0.0, width as f32, 0.0, height as f32, -1.0, 1.0),
            };

            self.attach_texture_to_canvas(&canvas, &canvas.texture, rebind_previous);

            Ok(canvas)
        }
    }

    pub(crate) fn bind_canvas(&mut self, canvas: Option<&Canvas>) {
        unsafe {
            let id = canvas.map(|x| x.framebuffer.id);

            if self.current_framebuffer != id {
                self.gl.bind_framebuffer(glow::FRAMEBUFFER, id);
                self.current_framebuffer = id;
            }
        }
    }

    pub(crate) fn attach_texture_to_canvas(
        &mut self,
        canvas: &Canvas,
        texture: &Texture,
        rebind_previous: bool,
    ) {
        unsafe {
            let previous_id = self.current_framebuffer;

            self.bind_canvas(Some(canvas));

            self.gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture.handle.borrow().id),
                0,
            );

            if rebind_previous {
                self.gl.bind_framebuffer(glow::FRAMEBUFFER, previous_id);
                self.current_framebuffer = previous_id;
            }
        }
    }

    pub(crate) fn get_default_filter_mode(&self) -> FilterMode {
        self.default_filter_mode
    }

    pub(crate) fn set_default_filter_mode(&mut self, filter_mode: FilterMode) {
        self.default_filter_mode = filter_mode;
    }
}

impl Drop for GLDevice {
    fn drop(&mut self) {
        unsafe {
            self.gl.bind_vertex_array(None);

            if let Some(va) = self.current_vertex_array {
                self.gl.delete_vertex_array(va);
            }
        }
    }
}

#[doc(hidden)]
impl From<FilterMode> for i32 {
    fn from(filter_mode: FilterMode) -> i32 {
        match filter_mode {
            FilterMode::Nearest => glow::NEAREST as i32,
            FilterMode::Linear => glow::LINEAR as i32,
        }
    }
}

#[doc(hidden)]
impl From<BufferUsage> for u32 {
    fn from(buffer_usage: BufferUsage) -> u32 {
        match buffer_usage {
            BufferUsage::StaticDraw => glow::STATIC_DRAW,
            BufferUsage::DynamicDraw => glow::DYNAMIC_DRAW,
        }
    }
}

#[doc(hidden)]
impl From<FrontFace> for u32 {
    fn from(front_face: FrontFace) -> u32 {
        match front_face {
            FrontFace::Clockwise => glow::CW,
            FrontFace::CounterClockwise => glow::CCW,
        }
    }
}

macro_rules! handle_impls {
    ($name:ty, $delete:ident) => {
        impl PartialEq for $name {
            fn eq(&self, other: &$name) -> bool {
                self.id == other.id
            }
        }

        impl Drop for $name {
            fn drop(&mut self) {
                unsafe {
                    self.gl.$delete(self.id);
                }
            }
        }
    };
}

#[derive(Debug)]
pub struct GLVertexBuffer {
    gl: Rc<GlContext>,

    id: BufferId,
    count: usize,
    stride: usize,
}

handle_impls!(GLVertexBuffer, delete_buffer);

#[derive(Debug)]
pub struct GLIndexBuffer {
    gl: Rc<GlContext>,

    id: BufferId,
    count: usize,
}

handle_impls!(GLIndexBuffer, delete_buffer);

#[derive(Debug)]
pub struct GLProgram {
    gl: Rc<GlContext>,

    id: ProgramId,
}

handle_impls!(GLProgram, delete_program);

#[derive(Debug)]
pub struct GLTexture {
    gl: Rc<GlContext>,

    id: TextureId,
    width: i32,
    height: i32,
    filter_mode: FilterMode,
}

handle_impls!(GLTexture, delete_texture);

impl GLTexture {
    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn filter_mode(&self) -> FilterMode {
        self.filter_mode
    }
}

#[derive(Debug)]
pub struct GLFramebuffer {
    gl: Rc<GlContext>,

    id: FramebufferId,
}

handle_impls!(GLFramebuffer, delete_framebuffer);

impl UniformValue for i32 {
    #[doc(hidden)]
    unsafe fn set_uniform(&self, shader: &Shader, location: Option<u32>) {
        shader.handle.gl.uniform_1_i32(location, *self);
    }
}

impl UniformValue for f32 {
    #[doc(hidden)]
    unsafe fn set_uniform(&self, shader: &Shader, location: Option<u32>) {
        shader.handle.gl.uniform_1_f32(location, *self);
    }
}

impl UniformValue for Mat4 {
    #[doc(hidden)]
    unsafe fn set_uniform(&self, shader: &Shader, location: Option<u32>) {
        let slice = self.as_slice();

        // I don't think there's a scenario where this wouldn't be true,
        // but better safe than sorry.
        assert_eq!(slice.len(), 16);

        let array_ref = slice.as_ptr() as *const [f32; 16];

        shader
            .handle
            .gl
            .uniform_matrix_4_f32_slice(location, false, &*array_ref);
    }
}

impl<'a, T> UniformValue for &'a T
where
    T: UniformValue,
{
    #[doc(hidden)]
    unsafe fn set_uniform(&self, shader: &Shader, location: Option<u32>) {
        (**self).set_uniform(shader, location);
    }
}
