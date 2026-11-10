mod common;

#[test]
fn test_rgba8unorm_target_format() {
    let (device, _) = match common::create_test_device() {
        Some(dq) => dq,
        None => {
            eprintln!("Skipping test: no GPU adapter found");
            return;
        }
    };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Rgba8Unorm Target"),
        size: wgpu::Extent3d {
            width: 128,
            height: 128,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    assert_eq!(texture.format(), wgpu::TextureFormat::Rgba8Unorm);
    assert_eq!(texture.width(), 128);
    assert_eq!(texture.height(), 128);
}

#[test]
fn test_instanced_draw_first_instance_and_instance_index() {
    let (device, queue) = match common::create_test_device() {
        Some(dq) => dq,
        None => {
            eprintln!("Skipping test: no GPU adapter found");
            return;
        }
    };

    let shader_source = r#"
        struct VertexOutput {
            @builtin(position) position: vec4<f32>,
            @location(0) out_val: vec4<f32>,
        };

        struct InstanceInput {
            @location(0) attr_val: f32,
        };

        @vertex
        fn vs_main(
            @builtin(vertex_index) v_idx: u32,
            @builtin(instance_index) inst_idx: u32,
            instance: InstanceInput,
        ) -> VertexOutput {
            var out: VertexOutput;
            let x = select(-1.0, 3.0, v_idx == 1u);
            let y = select(-1.0, 3.0, v_idx == 2u);
            out.position = vec4<f32>(x, y, 0.0, 1.0);

            // Red = inst_idx / 100.0, Green = attr_val / 100.0
            out.out_val = vec4<f32>(f32(inst_idx) / 100.0, instance.attr_val / 100.0, 0.0, 1.0);
            return out;
        }

        @fragment
        fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
            return in.out_val;
        }
    "#;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Test Shader"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Test Pipeline Layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Test Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: 4,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 0,
                    shader_location: 0,
                }],
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    // Populate instance buffer with 20 entries: [0.0, 1.0, ..., 19.0]
    let instance_data: Vec<f32> = (0..20).map(|i| i as f32).collect();
    use wgpu::util::DeviceExt;
    let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Instance Buffer"),
        contents: bytemuck::cast_slice(&instance_data),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let target_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Target Texture"),
        size: wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target_texture.create_view(&wgpu::TextureViewDescriptor::default());

    // Draw only instance 10 (range: 10..11)
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Test Encoder"),
    });
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Test Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        rpass.set_pipeline(&render_pipeline);
        rpass.set_vertex_buffer(0, instance_buffer.slice(..));
        rpass.draw(0..3, 10..11);
    }
    queue.submit(Some(encoder.finish()));

    let img = common::read_texture_to_image(&device, &queue, &target_texture, 16, 16);
    let pixel = img.get_pixel(8, 8);

    // Red = 10 / 100 * 255 ~= 25.5 -> 25 or 26
    // Green = 10 / 100 * 255 ~= 25.5 -> 25 or 26
    let expected_val = (10.0f32 / 100.0f32 * 255.0f32).round() as u8;
    assert!(
        (pixel[0] as i16 - expected_val as i16).abs() <= 1,
        "Red channel (@builtin(instance_index)) expected ~{}, got {}",
        expected_val,
        pixel[0]
    );
    assert!(
        (pixel[1] as i16 - expected_val as i16).abs() <= 1,
        "Green channel (Instance attribute) expected ~{}, got {}",
        expected_val,
        pixel[1]
    );
}
