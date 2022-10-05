use std::{
    env::args,
    fs::read_to_string,
    time::Instant,
};

use wgpu::{
    Backends, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingType, BufferBindingType, BufferUsages, Color, CommandEncoderDescriptor, Device,
    DeviceDescriptor, Features, FragmentState, Instance, Limits, LoadOp, MultisampleState,
    Operations, PipelineLayoutDescriptor, PowerPreference, PresentMode, PrimitiveState,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor,
    RequestAdapterOptions, ShaderModuleDescriptor, ShaderSource, ShaderStages, Surface,
    SurfaceConfiguration, TextureUsages, TextureViewDescriptor,
    util::{BufferInitDescriptor, DeviceExt}, VertexState,
};
use winit::{
    event::*,
    event_loop,
    window::WindowBuilder,
};

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    mouse: [f32; 2],
    resolution: [f32; 2],
    time: f32,
    padding: f32,
}

fn main() {
    pollster::block_on(run());
}

async fn run() {
    env_logger::init();

    // context for retrieving events from the system
    let event_loop = event_loop::EventLoop::new();

    // register a new window within the context
    let window = WindowBuilder::new()
        .with_title("Horus")
        .build(&event_loop)
        .unwrap();
    let size = window.inner_size();

    // wgpu
    let instance = Instance::new(Backends::all());

    // winit window -> wgpu window
    let mut surface = unsafe { instance.create_surface(&window) };

    // graphics card
    let adapter = instance.request_adapter(
        &RequestAdapterOptions {
            power_preference: PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        },
    ).await.unwrap();

    // device - logical representation of the graphics card
    // queue - how we assign work to the device
    let (device, queue) = adapter.request_device(
        &DeviceDescriptor {
            features: Features::empty(),
            limits: Limits::default(),
            label: None,
        },
        None,
    ).await.unwrap();

    // configure the surface
    let mut config = SurfaceConfiguration {
        usage: TextureUsages::RENDER_ATTACHMENT,
        format: surface.get_supported_formats(&adapter)[0],
        width: size.width,
        height: size.height,
        present_mode: PresentMode::Fifo, // basically vsync
    };
    surface.configure(&device, &config);

    // vertex shader
    let vertex_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: None,
        source: ShaderSource::Wgsl(include_str!("vertex.wgsl").into()),
    });

    // fragment shader
    let mut fragment_path = "./src/fragment.wgsl".to_string();
    if args().len() > 1 {
        fragment_path = args().nth(1).unwrap();
    }
    let fragment_source = read_to_string(&fragment_path).unwrap();
    let fragment_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: None,
        source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(&fragment_source)),
    });

    // uniform data to be sent to the shaders
    let mut uniforms = Uniforms { mouse: [0., 0.], resolution: [size.width as _, size.height as _], time: 0., padding: 0. };
    let time = Instant::now();
    let uniforms_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&uniforms),
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });
    let uniforms_buffer_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: None,
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::FRAGMENT,
            count: None,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
        }],
    });
    let uniforms_buffer_bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &uniforms_buffer_layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: uniforms_buffer.as_entire_binding(),
        }],
    });

    // determines which resources are bound to the pipeline
    let render_pipeline_layout =
        device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&uniforms_buffer_layout], // just our uniforms
            push_constant_ranges: &[],
        });

    // represents all stages of the rendering process
    let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: None,
        layout: Some(&render_pipeline_layout),
        vertex: VertexState {
            module: &vertex_shader,
            entry_point: "vs_main",
            buffers: &[],
        },
        fragment: Some(FragmentState {
            module: &fragment_shader,
            entry_point: "fs_main",
            targets: &[Some(config.format.into())],
        }),
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState::default(),
        multiview: None,
    });

    // continuously poll window events from the system
    event_loop.run(move |event, _, control_flow| {
        *control_flow = event_loop::ControlFlow::Poll;
        match event {
            Event::MainEventsCleared => window.request_redraw(),
            Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == window.id() => {
                match event {
                    WindowEvent::CloseRequested
                    | WindowEvent::KeyboardInput {
                        input:
                        KeyboardInput {
                            state: ElementState::Pressed,
                            virtual_keycode: Some(VirtualKeyCode::Escape),
                            ..
                        },
                        ..
                    } => *control_flow = event_loop::ControlFlow::Exit,
                    WindowEvent::Resized(physical_size) => {
                        resize(&device, &mut surface, &mut config, *physical_size, &mut uniforms);
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        resize(&device, &mut surface, &mut config, **new_inner_size, &mut uniforms);
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        // update uniforms
                        uniforms.mouse = [position.x as _, position.y as _];
                    }
                    _ => {}
                }
            }
            Event::RedrawRequested(window_id) if window_id == window.id() => {
                // get a SurfaceTexture to render to
                let output = surface.get_current_texture().unwrap();
                let view = output.texture.create_view(&TextureViewDescriptor::default());

                // update uniforms
                uniforms.time = time.elapsed().as_secs_f32();
                queue.write_buffer(&uniforms_buffer, 0, bytemuck::bytes_of(&uniforms));

                // the encoder will create a command buffer to send to the device
                let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor { label: None });

                {
                    let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: Operations {
                                load: LoadOp::Clear(Color::BLACK),
                                store: true,
                            },
                        })],
                        depth_stencil_attachment: None,
                    });
                    render_pass.set_pipeline(&render_pipeline);
                    render_pass.set_bind_group(0, &uniforms_buffer_bind_group, &[]);
                    render_pass.draw(0..3, 0..1);
                }

                // send it to the device for rendering
                queue.submit(std::iter::once(encoder.finish()));
                output.present();
            }
            _ => {}
        }
    });
}

// update uniforms, config and then resize surface to fit the window
fn resize(device: &Device, surface: &mut Surface, config: &mut SurfaceConfiguration, new_size: winit::dpi::PhysicalSize<u32>, uniforms: &mut Uniforms) {
    if new_size.width > 0 && new_size.height > 0 {
        config.width = new_size.width;
        config.height = new_size.height;
        uniforms.resolution = [new_size.width as _, new_size.height as _];
        surface.configure(device, config);
    }
}