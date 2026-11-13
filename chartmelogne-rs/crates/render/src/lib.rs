//! Candle and grid renderer ported from TypeScript/WebGL2 to wgpu 30 + WGSL.

use wgpu::util::DeviceExt;

/// 12 vertices: 2 triangles for wick (part = 0.0), 2 triangles for body (part = 1.0)
/// Format: [xSide (-0.5..0.5), yAnchor (0.0..1.0), part (0.0 or 1.0)]
pub const UNIT_VERTICES: [f32; 36] = [
    // Wick quad (part = 0.0)
    -0.5, 0.0, 0.0,
     0.5, 0.0, 0.0,
     0.5, 1.0, 0.0,
    -0.5, 0.0, 0.0,
     0.5, 1.0, 0.0,
    -0.5, 1.0, 0.0,

    // Body quad (part = 1.0)
    -0.5, 0.0, 1.0,
     0.5, 0.0, 1.0,
     0.5, 1.0, 1.0,
    -0.5, 0.0, 1.0,
     0.5, 1.0, 1.0,
    -0.5, 1.0, 1.0,
];

pub const CANDLE_SHADER_SRC: &str = include_str!("candle.wgsl");
pub const GRID_SHADER_SRC: &str = include_str!("grid.wgsl");
pub const VOLUME_SHADER_SRC: &str = include_str!("volume.wgsl");
pub const LINE_SHADER_SRC: &str = include_str!("line.wgsl");
pub const INDICATOR_SHADER_SRC: &str = include_str!("indicator.wgsl");

pub const HISTOGRAM_UNIT_VERTICES: [f32; 12] = [
    -0.5, 0.0,
     0.5, 0.0,
     0.5, 1.0,
    -0.5, 0.0,
     0.5, 1.0,
    -0.5, 1.0,
];

/// 64-byte uniform buffer layout matching std140 / WGSL rules
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CandleUniforms {
    pub viewport_px: [f32; 2], // offset 0
    pub px_per_bar: f32,       // offset 8
    pub from_index: f32,       // offset 12
    pub y_scale: f32,          // offset 16
    pub y_offset: f32,         // offset 20
    pub bars_per_node: f32,    // offset 24
    pub dpr: f32,              // offset 28
    pub up_color: [f32; 4],    // offset 32
    pub down_color: [f32; 4],  // offset 48
    pub body_width: f32,       // offset 64
    pub wick_width: f32,       // offset 68
    pub _pad: [f32; 2],        // offset 72
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GridUniforms {
    pub viewport_px: [f32; 2],
    pub _padding: [f32; 2],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VolumeUniforms {
    pub viewport_px: [f32; 2],
    pub px_per_bar: f32,
    pub from_index: f32,
    pub max_volume: f32,
    pub height_ratio: f32,
    pub bar_width: f32,
    pub max_h: f32,
    pub up_color: [f32; 4],
    pub down_color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineUniforms {
    pub viewport_px: [f32; 2],
    pub px_per_bar: f32,
    pub from_index: f32,
    pub y_scale: f32,
    pub y_offset: f32,
    pub dpr: f32,
    pub line_width: f32,
    pub is_area: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
    pub color: [f32; 4],
    pub bottom_color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct IndicatorUniforms {
    pub viewport_px: [f32; 2],
    pub px_per_bar: f32,
    pub from_index: f32,
    pub y_scale: f32,
    pub y_offset: f32,
    pub dpr: f32,
    pub line_width: f32,
    pub color: [f32; 4],
}

pub struct IndicatorGpuBuffer {
    pub id: String,
    pub color: [f32; 4],
    pub line_width: f32,
    pub buffer: wgpu::Buffer,
    pub capacity: usize,
    pub count: usize,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

/// Rendering parameters passed per frame
#[derive(Clone, Debug)]
pub struct ViewParams {
    pub from_index: f32,
    pub px_per_bar: f32,
    pub y_min: f32,
    pub y_max: f32,
    pub base_price: f32,
    pub count: usize,
    pub dpr: f32,
    pub up_color: [f32; 4],
    pub down_color: [f32; 4],
    pub clear_color: Option<wgpu::Color>,
    pub grid_color: Option<[f32; 4]>,
    pub horizontal_grid_y: Vec<f32>,
    pub vertical_grid_x: Vec<f32>,
    pub show_volume: bool,
    pub volume_max: f32,
    pub volume_height_ratio: f32,
    pub volume_up_color: [f32; 4],
    pub volume_down_color: [f32; 4],
    pub allow_zoom: bool,
    pub show_axes: bool,
    pub chart_type: chartmelogne_core::types::ChartType,
    pub show_indicators: bool,
}

impl Default for ViewParams {
    fn default() -> Self {
        Self {
            from_index: 0.0,
            px_per_bar: 10.0,
            y_min: 0.0,
            y_max: 100.0,
            base_price: 0.0,
            count: 0,
            dpr: 1.0,
            up_color: [0.03, 0.65, 0.55, 1.0],
            down_color: [0.95, 0.25, 0.33, 1.0],
            clear_color: None,
            grid_color: None,
            horizontal_grid_y: Vec::new(),
            vertical_grid_x: Vec::new(),
            show_volume: true,
            volume_max: 1.0,
            volume_height_ratio: 0.20,
            volume_up_color: [0.03, 0.65, 0.55, 0.35],
            volume_down_color: [0.95, 0.25, 0.33, 0.35],
            allow_zoom: true,
            show_axes: true,
            chart_type: chartmelogne_core::types::ChartType::Candlestick,
            show_indicators: false,
        }
    }
}

pub struct ChartRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,

    pipeline: wgpu::RenderPipeline,
    unit_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,

    grid_pipeline: wgpu::RenderPipeline,
    grid_uniform_buffer: wgpu::Buffer,
    grid_bind_group: wgpu::BindGroup,
    grid_vertex_buffer: wgpu::Buffer,
    grid_vertex_capacity: usize,
    grid_vertices: Vec<[f32; 2]>,

    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    instance_count: usize,

    volume_pipeline: wgpu::RenderPipeline,
    volume_unit_buffer: wgpu::Buffer,
    volume_uniform_buffer: wgpu::Buffer,
    volume_bind_group: wgpu::BindGroup,
    volume_buffer: wgpu::Buffer,
    volume_capacity: usize,
    volume_count: usize,

    line_pipeline: wgpu::RenderPipeline,
    line_uniform_buffer: wgpu::Buffer,
    line_bind_group: wgpu::BindGroup,
    area_uniform_buffer: wgpu::Buffer,
    area_bind_group: wgpu::BindGroup,

    indicator_pipeline: wgpu::RenderPipeline,
    indicator_bind_group_layout: wgpu::BindGroupLayout,
    indicators: Vec<IndicatorGpuBuffer>,

    plot_width: u32,
    plot_height: u32,
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
}

impl ChartRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        // 1. Candle pipeline
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Candle Shader"),
            source: wgpu::ShaderSource::Wgsl(CANDLE_SHADER_SRC.into()),
        });

        let uniform_size = std::mem::size_of::<CandleUniforms>() as u64;
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Candle Uniform Buffer"),
            size: uniform_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Candle Uniform BGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(uniform_size),
                    },
                    count: None,
                }],
            });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Candle Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Candle Pipeline Layout"),
            bind_group_layouts: &[Some(&uniform_bind_group_layout)],
            immediate_size: 0,
        });

        let unit_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Unit Vertex Buffer"),
            contents: bytemuck::cast_slice(&UNIT_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let initial_capacity = 1024usize;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Candle Instance Buffer"),
            size: (initial_capacity * 16) as u64,
            usage: wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let vertex_buffer_layouts = [
            // Buffer 0: Static unit geometry
            Some(wgpu::VertexBufferLayout {
                array_stride: 12,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                }],
            }),
            // Buffer 1: Dynamic instance OHLC (16 bytes per instance)
            Some(wgpu::VertexBufferLayout {
                array_stride: 16,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 1,
                }],
            }),
        ];

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Candle Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &vertex_buffer_layouts,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // 2. Grid pipeline
        let grid_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Grid Shader"),
            source: wgpu::ShaderSource::Wgsl(GRID_SHADER_SRC.into()),
        });

        let grid_uniform_size = std::mem::size_of::<GridUniforms>() as u64;
        let grid_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Grid Uniform Buffer"),
            size: grid_uniform_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let grid_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Grid Uniform BGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(grid_uniform_size),
                    },
                    count: None,
                }],
            });

        let grid_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Grid Uniform Bind Group"),
            layout: &grid_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: grid_uniform_buffer.as_entire_binding(),
            }],
        });

        let grid_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Grid Pipeline Layout"),
                bind_group_layouts: &[Some(&grid_bind_group_layout)],
                immediate_size: 0,
            });

        let grid_vertex_layouts = [Some(wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            }],
        })];

        let grid_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Grid Render Pipeline"),
            layout: Some(&grid_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &grid_shader,
                entry_point: Some("vs_grid"),
                compilation_options: Default::default(),
                buffers: &grid_vertex_layouts,
            },
            fragment: Some(wgpu::FragmentState {
                module: &grid_shader,
                entry_point: Some("fs_grid"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let grid_vertex_capacity = 256;
        let grid_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Grid Vertex Buffer"),
            size: (grid_vertex_capacity * 8) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // 3. Volume pipeline
        let volume_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Volume Shader"),
            source: wgpu::ShaderSource::Wgsl(VOLUME_SHADER_SRC.into()),
        });

        let volume_uniform_size = std::mem::size_of::<VolumeUniforms>() as u64;
        let volume_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Volume Uniform Buffer"),
            size: volume_uniform_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let volume_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Volume Uniform BGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(volume_uniform_size),
                    },
                    count: None,
                }],
            });

        let volume_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Volume Uniform BG"),
            layout: &volume_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: volume_uniform_buffer.as_entire_binding(),
            }],
        });

        let volume_unit_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Volume Unit Vertex Buffer"),
            contents: bytemuck::cast_slice(&HISTOGRAM_UNIT_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let volume_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Volume Instance Buffer"),
            size: (initial_capacity * 8) as u64,
            usage: wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let volume_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Volume Pipeline Layout"),
                bind_group_layouts: &[Some(&volume_bind_group_layout)],
                immediate_size: 0,
            });

        let volume_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Volume Render Pipeline"),
            layout: Some(&volume_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &volume_shader,
                entry_point: Some("vs_volume"),
                compilation_options: Default::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: 8,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        }],
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: 8,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 1,
                        }],
                    }),
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &volume_shader,
                entry_point: Some("fs_volume"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // 4. Line / Area pipeline
        let line_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Line Shader"),
            source: wgpu::ShaderSource::Wgsl(LINE_SHADER_SRC.into()),
        });

        let line_uniform_size = std::mem::size_of::<LineUniforms>() as u64;
        let line_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Line Uniform Buffer"),
            size: line_uniform_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let area_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Area Uniform Buffer"),
            size: line_uniform_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let line_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Line Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let line_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Line Bind Group"),
            layout: &line_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: line_uniform_buffer.as_entire_binding(),
            }],
        });

        let area_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Area Bind Group"),
            layout: &line_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: area_uniform_buffer.as_entire_binding(),
            }],
        });

        let line_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Line Pipeline Layout"),
                bind_group_layouts: &[Some(&line_bind_group_layout)],
                immediate_size: 0,
            });

        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Line Render Pipeline"),
            layout: Some(&line_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &line_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: 16,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 0,
                            shader_location: 0,
                        }],
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: 16,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 0,
                            shader_location: 1,
                        }],
                    }),
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &line_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // 5. Indicator pipeline
        let indicator_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Indicator Shader"),
            source: wgpu::ShaderSource::Wgsl(INDICATOR_SHADER_SRC.into()),
        });

        let indicator_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Indicator Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let indicator_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Indicator Pipeline Layout"),
                bind_group_layouts: &[Some(&indicator_bind_group_layout)],
                immediate_size: 0,
            });

        let indicator_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Indicator Render Pipeline"),
            layout: Some(&indicator_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &indicator_shader,
                entry_point: Some("vs_indicator"),
                compilation_options: Default::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: 4,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 0,
                            shader_location: 0,
                        }],
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: 4,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 0,
                            shader_location: 1,
                        }],
                    }),
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &indicator_shader,
                entry_point: Some("fs_indicator"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // 6. Texture setup
        let plot_width = 1;
        let plot_height = 1;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Plot Texture"),
            size: wgpu::Extent3d {
                width: plot_width,
                height: plot_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            device: device.clone(),
            queue: queue.clone(),
            format,
            pipeline,
            unit_buffer,
            uniform_buffer,
            uniform_bind_group,
            grid_pipeline,
            grid_uniform_buffer,
            grid_bind_group,
            grid_vertex_buffer,
            grid_vertex_capacity,
            grid_vertices: Vec::with_capacity(256),
            instance_buffer,
            instance_capacity: initial_capacity,
            instance_count: 0,
            volume_pipeline,
            volume_unit_buffer,
            volume_uniform_buffer,
            volume_bind_group,
            volume_buffer,
            volume_capacity: initial_capacity,
            volume_count: 0,
            line_pipeline,
            line_uniform_buffer,
            line_bind_group,
            area_uniform_buffer,
            area_bind_group,
            indicator_pipeline,
            indicator_bind_group_layout,
            indicators: Vec::new(),
            plot_width,
            plot_height,
            texture,
            texture_view,
        }
    }

    /// Resize plot texture in physical pixels
    pub fn resize(&mut self, w_px: u32, h_px: u32) {
        let w = w_px.max(1);
        let h = h_px.max(1);
        if self.plot_width == w && self.plot_height == h {
            return;
        }

        self.plot_width = w;
        self.plot_height = h;

        self.texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Plot Texture"),
            size: wgpu::Extent3d {
                width: self.plot_width,
                height: self.plot_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.texture_view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
    }

    /// Upload entire OHLC buffer: `ohlc` has 4 floats per bar [o, h, l, c] relative to base_price.
    pub fn upload_all(&mut self, ohlc: &[f32], count: usize) {
        if count > self.instance_capacity {
            let mut new_cap = self.instance_capacity.max(1024);
            while new_cap < count {
                new_cap *= 2;
            }
            self.instance_capacity = new_cap;
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Candle Instance Buffer"),
                size: (new_cap * 16) as u64,
                usage: wgpu::BufferUsages::VERTEX
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
        }

        if count > 0 && ohlc.len() >= count * 4 {
            let slice = &ohlc[..count * 4];
            self.queue
                .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(slice));
        }
        self.instance_count = count;
    }

    /// Append a new bar at `index` ([o, h, l, c] relative to base_price).
    /// If index >= capacity, doubles capacity and copies previous data on GPU.
    pub fn append(&mut self, bar: &[f32; 4], index: usize) {
        if index >= self.instance_capacity {
            let mut new_cap = self.instance_capacity.max(1024);
            while new_cap <= index {
                new_cap *= 2;
            }
            let new_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Candle Instance Buffer (Expanded)"),
                size: (new_cap * 16) as u64,
                usage: wgpu::BufferUsages::VERTEX
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });

            // Copy existing data on GPU
            if self.instance_count > 0 {
                let copy_bytes = (self.instance_count * 16) as u64;
                let mut encoder = self
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Instance Buffer Expansion Copy"),
                    });
                encoder.copy_buffer_to_buffer(&self.instance_buffer, 0, &new_buffer, 0, copy_bytes);
                self.queue.submit(Some(encoder.finish()));
            }

            self.instance_buffer = new_buffer;
            self.instance_capacity = new_cap;
        }

        let offset = (index * 16) as u64;
        self.queue
            .write_buffer(&self.instance_buffer, offset, bytemuck::cast_slice(bar));

        if index + 1 > self.instance_count {
            self.instance_count = index + 1;
        }
    }

    /// Update the bar at `index` in-place (16 bytes write).
    pub fn update_last(&mut self, bar: &[f32; 4], index: usize) {
        if index < self.instance_capacity {
            let offset = (index * 16) as u64;
            self.queue
                .write_buffer(&self.instance_buffer, offset, bytemuck::cast_slice(bar));
            if index + 1 > self.instance_count {
                self.instance_count = index + 1;
            }
        }
    }

    /// Upload volume buffer: `volume_data` has 2 floats per bar [volume, is_up (1.0 or 0.0)]
    pub fn upload_volume(&mut self, volume_data: &[f32], count: usize) {
        if count > self.volume_capacity {
            let mut new_cap = self.volume_capacity.max(1024);
            while new_cap < count {
                new_cap *= 2;
            }
            self.volume_capacity = new_cap;
            self.volume_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Volume Instance Buffer"),
                size: (new_cap * 8) as u64,
                usage: wgpu::BufferUsages::VERTEX
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
        }

        if count > 0 && volume_data.len() >= count * 2 {
            let slice = &volume_data[..count * 2];
            self.queue
                .write_buffer(&self.volume_buffer, 0, bytemuck::cast_slice(slice));
        }
        self.volume_count = count;
    }

    /// Append a volume bar at `index` ([volume, is_up]).
    pub fn append_volume(&mut self, bar: &[f32; 2], index: usize) {
        if index >= self.volume_capacity {
            let mut new_cap = self.volume_capacity.max(1024);
            while new_cap <= index {
                new_cap *= 2;
            }
            let new_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Volume Instance Buffer (Expanded)"),
                size: (new_cap * 8) as u64,
                usage: wgpu::BufferUsages::VERTEX
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });

            if self.volume_count > 0 {
                let copy_bytes = (self.volume_count * 8) as u64;
                let mut encoder = self
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Volume Buffer Expansion Copy"),
                    });
                encoder.copy_buffer_to_buffer(&self.volume_buffer, 0, &new_buffer, 0, copy_bytes);
                self.queue.submit(Some(encoder.finish()));
            }

            self.volume_buffer = new_buffer;
            self.volume_capacity = new_cap;
        }

        let offset = (index * 8) as u64;
        self.queue
            .write_buffer(&self.volume_buffer, offset, bytemuck::cast_slice(bar));

        if index + 1 > self.volume_count {
            self.volume_count = index + 1;
        }
    }

    /// Update the volume bar at `index` in-place.
    pub fn update_last_volume(&mut self, bar: &[f32; 2], index: usize) {
        if index < self.volume_capacity {
            let offset = (index * 8) as u64;
            self.queue
                .write_buffer(&self.volume_buffer, offset, bytemuck::cast_slice(bar));
            if index + 1 > self.volume_count {
                self.volume_count = index + 1;
            }
        }
    }

    /// Upload or update a host-calculated indicator series (4 bytes per bar relative to base_price).
    pub fn upload_indicator(
        &mut self,
        id: &str,
        color: [f32; 4],
        line_width: f32,
        values_relative: &[f32],
        count: usize,
    ) {
        let needed_cap = (count + 1).max(1024);
        let needed_bytes = (needed_cap * 4) as u64;

        if let Some(pos) = self.indicators.iter().position(|i| i.id == id) {
            let ind = &mut self.indicators[pos];
            ind.color = color;
            ind.line_width = line_width;

            if needed_cap > ind.capacity {
                ind.buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("Indicator Buffer {}", id)),
                    size: needed_bytes,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                ind.capacity = needed_cap;
            }

            if count > 0 && values_relative.len() >= count {
                self.queue.write_buffer(
                    &ind.buffer,
                    0,
                    bytemuck::cast_slice(&values_relative[..count]),
                );
            }
            ind.count = count;
        } else {
            let uniform_size = std::mem::size_of::<IndicatorUniforms>() as u64;
            let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("Indicator Uniform Buffer {}", id)),
                size: uniform_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("Indicator Bind Group {}", id)),
                layout: &self.indicator_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }],
            });

            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("Indicator Buffer {}", id)),
                size: needed_bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            if count > 0 && values_relative.len() >= count {
                self.queue.write_buffer(
                    &buffer,
                    0,
                    bytemuck::cast_slice(&values_relative[..count]),
                );
            }

            self.indicators.push(IndicatorGpuBuffer {
                id: id.to_string(),
                color,
                line_width,
                buffer,
                capacity: needed_cap,
                count,
                uniform_buffer,
                bind_group,
            });
        }
    }

    /// Remove an indicator series from the GPU renderer.
    pub fn remove_indicator(&mut self, id: &str) -> bool {
        if let Some(pos) = self.indicators.iter().position(|i| i.id == id) {
            self.indicators.remove(pos);
            true
        } else {
            false
        }
    }

    /// Remove all uploaded indicators from the GPU renderer.
    pub fn clear_indicators(&mut self) {
        self.indicators.clear();
    }

    pub fn indicators(&self) -> &[IndicatorGpuBuffer] {
        &self.indicators
    }

    /// Render grid and candles into the target texture.
    pub fn render(&mut self, encoder: &mut wgpu::CommandEncoder, view: &ViewParams) {
        if self.plot_width == 0 || self.plot_height == 0 {
            return;
        }

        let load_op = match view.clear_color {
            Some(color) => wgpu::LoadOp::Clear(color),
            None => wgpu::LoadOp::Load,
        };

        // --- GRID PASS SETUP ---
        let mut grid_vertex_count = 0usize;
        if let Some(grid_col) = view.grid_color {
            self.grid_vertices.clear();
            let pw = self.plot_width as f32;
            let ph = self.plot_height as f32;

            // Horizontal lines
            for &y in &view.horizontal_grid_y {
                let y0 = (y - 0.5).floor();
                let y1 = y0 + 1.0;
                self.grid_vertices.extend_from_slice(&[
                    [0.0, y0],
                    [pw, y0],
                    [pw, y1],
                    [0.0, y0],
                    [pw, y1],
                    [0.0, y1],
                ]);
            }

            // Vertical lines
            for &x in &view.vertical_grid_x {
                let x0 = (x - 0.5).floor();
                let x1 = x0 + 1.0;
                self.grid_vertices.extend_from_slice(&[
                    [x0, 0.0],
                    [x1, 0.0],
                    [x1, ph],
                    [x0, 0.0],
                    [x1, ph],
                    [x0, ph],
                ]);
            }

            grid_vertex_count = self.grid_vertices.len();
            if grid_vertex_count > 0 {
                if grid_vertex_count > self.grid_vertex_capacity {
                    let new_cap = grid_vertex_count.next_power_of_two();
                    self.grid_vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Grid Vertex Buffer (Expanded)"),
                        size: (new_cap * 8) as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    self.grid_vertex_capacity = new_cap;
                }
                self.queue.write_buffer(
                    &self.grid_vertex_buffer,
                    0,
                    bytemuck::cast_slice(&self.grid_vertices),
                );

                let grid_uniforms = GridUniforms {
                    viewport_px: [pw, ph],
                    _padding: [0.0, 0.0],
                    color: grid_col,
                };
                self.queue.write_buffer(
                    &self.grid_uniform_buffer,
                    0,
                    bytemuck::bytes_of(&grid_uniforms),
                );
            }
        }

        // --- CANDLE PASS SETUP ---
        let bars_visible = if view.px_per_bar > 0.0 {
            (self.plot_width as f32) / view.px_per_bar
        } else {
            0.0
        };

        let first = ((view.from_index - 2.0).floor() as isize).max(0) as u32;
        let last = ((view.from_index + bars_visible + 2.0).ceil() as isize)
            .min(view.count as isize - 1)
            .max(0) as u32;

        let should_draw_candles = view.chart_type == chartmelogne_core::types::ChartType::Candlestick
            && view.count > 0
            && first <= last
            && (first as usize) < view.count;

        let should_draw_volume = view.show_volume
            && self.volume_count > 0
            && first <= last
            && (first as usize) < self.volume_count;

        if should_draw_volume {
            let bar_width = if view.px_per_bar < 3.0 * view.dpr {
                1.0
            } else {
                (view.px_per_bar * 0.7).clamp(1.0, 30.0 * view.dpr)
            };
            let height_ratio = view.volume_height_ratio.clamp(0.01, 1.0);
            let max_h = (self.plot_height as f32) * height_ratio;

            let vol_uniforms = VolumeUniforms {
                viewport_px: [self.plot_width as f32, self.plot_height as f32],
                px_per_bar: view.px_per_bar,
                from_index: view.from_index,
                max_volume: view.volume_max.max(1.0),
                height_ratio,
                bar_width,
                max_h,
                up_color: view.volume_up_color,
                down_color: view.volume_down_color,
            };
            self.queue
                .write_buffer(&self.volume_uniform_buffer, 0, bytemuck::bytes_of(&vol_uniforms));
        }

        if should_draw_candles {
            let y_span = view.y_max - view.y_min;
            let y_scale = if y_span > 0.0 {
                (self.plot_height as f32) / y_span
            } else {
                1.0
            };
            let y_offset = view.y_min - view.base_price;
            let bars_per_node = 1.0;

            let (body_width, wick_width) = if view.px_per_bar < 3.0 * view.dpr {
                (0.0, 1.0)
            } else {
                let bw = (view.px_per_bar * 0.7).clamp(1.0, 30.0 * view.dpr);
                let ww = 1.0f32.max((1.0f32.max(0.1 * bw) + 0.5).floor());
                (bw, ww)
            };

            let uniforms = CandleUniforms {
                viewport_px: [self.plot_width as f32, self.plot_height as f32],
                px_per_bar: view.px_per_bar,
                from_index: view.from_index,
                y_scale,
                y_offset,
                bars_per_node,
                dpr: view.dpr,
                up_color: view.up_color,
                down_color: view.down_color,
                body_width,
                wick_width,
                _pad: [0.0, 0.0],
            };

            self.queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        }

        // --- LINE / AREA PASS SETUP ---
        let seg_count = if view.count >= 2 { view.count - 1 } else { 0 };
        let last_seg = ((view.from_index + bars_visible + 2.0).ceil() as isize)
            .min(seg_count as isize - 1)
            .max(0) as u32;

        let should_draw_line = (view.chart_type == chartmelogne_core::types::ChartType::Line
            || view.chart_type == chartmelogne_core::types::ChartType::Area)
            && seg_count > 0
            && first <= last_seg
            && (first as usize) < seg_count;

        if should_draw_line {
            let y_span = view.y_max - view.y_min;
            let y_scale = if y_span > 0.0 {
                (self.plot_height as f32) / y_span
            } else {
                1.0
            };
            let y_offset = view.y_min - view.base_price;

            let line_uniforms = LineUniforms {
                viewport_px: [self.plot_width as f32, self.plot_height as f32],
                px_per_bar: view.px_per_bar,
                from_index: view.from_index,
                y_scale,
                y_offset,
                dpr: view.dpr,
                line_width: 2.0,
                is_area: 0.0,
                _pad0: 0.0,
                _pad1: 0.0,
                _pad2: 0.0,
                color: [0.1608, 0.3843, 1.0, 1.0],
                bottom_color: [0.0, 0.0, 0.0, 0.0],
            };
            self.queue.write_buffer(
                &self.line_uniform_buffer,
                0,
                bytemuck::bytes_of(&line_uniforms),
            );

            if view.chart_type == chartmelogne_core::types::ChartType::Area {
                let area_uniforms = LineUniforms {
                    viewport_px: [self.plot_width as f32, self.plot_height as f32],
                    px_per_bar: view.px_per_bar,
                    from_index: view.from_index,
                    y_scale,
                    y_offset,
                    dpr: view.dpr,
                    line_width: 0.0,
                    is_area: 1.0,
                    _pad0: 0.0,
                    _pad1: 0.0,
                    _pad2: 0.0,
                    color: [0.1608, 0.3843, 1.0, 0.30],
                    bottom_color: [0.1608, 0.3843, 1.0, 0.0],
                };
                self.queue.write_buffer(
                    &self.area_uniform_buffer,
                    0,
                    bytemuck::bytes_of(&area_uniforms),
                );
            }
        }

        // --- INDICATOR PASS SETUP ---
        if view.show_indicators && !self.indicators.is_empty() {
            let y_span = view.y_max - view.y_min;
            let y_scale = if y_span > 0.0 {
                (self.plot_height as f32) / y_span
            } else {
                1.0
            };
            let y_offset = view.y_min - view.base_price;

            for ind in &self.indicators {
                let ind_uniforms = IndicatorUniforms {
                    viewport_px: [self.plot_width as f32, self.plot_height as f32],
                    px_per_bar: view.px_per_bar,
                    from_index: view.from_index,
                    y_scale,
                    y_offset,
                    dpr: view.dpr,
                    line_width: ind.line_width,
                    color: ind.color,
                };
                self.queue.write_buffer(
                    &ind.uniform_buffer,
                    0,
                    bytemuck::bytes_of(&ind_uniforms),
                );
            }
        }

        // --- COMBINED RENDER PASS ---
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Chart Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: load_op,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // 1. Draw Grid Lines
            if grid_vertex_count > 0 && view.show_axes {
                rpass.set_pipeline(&self.grid_pipeline);
                rpass.set_bind_group(0, &self.grid_bind_group, &[]);
                rpass.set_vertex_buffer(0, self.grid_vertex_buffer.slice(..));
                rpass.draw(0..grid_vertex_count as u32, 0..1);
            }

            // 2. Draw Volume Histogram
            if should_draw_volume {
                rpass.set_pipeline(&self.volume_pipeline);
                rpass.set_bind_group(0, &self.volume_bind_group, &[]);
                rpass.set_vertex_buffer(0, self.volume_unit_buffer.slice(..));
                rpass.set_vertex_buffer(1, self.volume_buffer.slice(..));
                rpass.draw(0..6, first..(last + 1));
            }

            // 3. Draw Candles
            if should_draw_candles {
                rpass.set_pipeline(&self.pipeline);
                rpass.set_bind_group(0, &self.uniform_bind_group, &[]);
                rpass.set_vertex_buffer(0, self.unit_buffer.slice(..));
                rpass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                rpass.draw(0..12, first..(last + 1));
            }

            // 4. Draw Line / Area
            if should_draw_line {
                if view.chart_type == chartmelogne_core::types::ChartType::Area {
                    rpass.set_pipeline(&self.line_pipeline);
                    rpass.set_bind_group(0, &self.area_bind_group, &[]);
                    rpass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                    rpass.set_vertex_buffer(1, self.instance_buffer.slice(16..));
                    rpass.draw(0..6, first..(last_seg + 1));
                }

                rpass.set_pipeline(&self.line_pipeline);
                rpass.set_bind_group(0, &self.line_bind_group, &[]);
                rpass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                rpass.set_vertex_buffer(1, self.instance_buffer.slice(16..));
                rpass.draw(0..6, first..(last_seg + 1));
            }

            // 5. Draw Host Indicators
            if view.show_indicators {
                for ind in &self.indicators {
                    let seg_count = if ind.count >= 2 { ind.count - 1 } else { 0 };
                    let last_seg = ((view.from_index + bars_visible + 2.0).ceil() as isize)
                        .min(seg_count as isize - 1)
                        .max(0) as u32;

                    if seg_count > 0 && first <= last_seg && (first as usize) < seg_count {
                        rpass.set_pipeline(&self.indicator_pipeline);
                        rpass.set_bind_group(0, &ind.bind_group, &[]);
                        rpass.set_vertex_buffer(0, ind.buffer.slice(..));
                        rpass.set_vertex_buffer(1, ind.buffer.slice(4..));
                        rpass.draw(0..6, first..(last_seg + 1));
                    }
                }
            }
        }
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.texture_view
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn instance_capacity(&self) -> usize {
        self.instance_capacity
    }

    pub fn instance_count(&self) -> usize {
        self.instance_count
    }

    pub fn instance_buffer(&self) -> &wgpu::Buffer {
        &self.instance_buffer
    }

    pub fn plot_width(&self) -> u32 {
        self.plot_width
    }

    pub fn plot_height(&self) -> u32 {
        self.plot_height
    }
}
