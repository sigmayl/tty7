use super::*;
use wgpu::util::DeviceExt as _;

#[test]
fn window_clip_shaders_validate() {
    let base = include_str!("shaders.wgsl");
    for source in [
        base.to_owned(),
        format!(
            "enable dual_source_blending;\n{base}\n{}",
            include_str!("shaders_subpixel.wgsl")
        ),
    ] {
        let module = wgpu::naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}

#[test]
fn window_clip_cuts_pixels_without_insetting_content() -> anyhow::Result<()> {
    let (device, queue) = gpui::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            display: None,
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await?;
        let pair = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("window_clip_test"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await?;
        Ok::<_, anyhow::Error>(pair)
    })?;
    let layouts = WgpuRenderer::create_bind_group_layouts(&device);
    // Build the actual production pipelines, including text, images, paths,
    // and shadows, so a shader/host uniform mismatch fails this test too.
    let pipelines = WgpuRenderer::create_pipelines(
        &device,
        &layouts,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::CompositeAlphaMode::PreMultiplied,
        1,
        false,
    );
    for scale in [1., 2.] {
        for rounded in [true, false] {
            let side = (96. * scale) as u32;
            let bounds = Bounds {
                origin: gpui::point(ScaledPixels(12. * scale), ScaledPixels(12. * scale)),
                size: gpui::size(ScaledPixels(72. * scale), ScaledPixels(72. * scale)),
            };
            let globals = GlobalParams {
                viewport_size: [side as f32; 2],
                premultiplied_alpha: 1,
                pad: 0,
                window_clip_bounds: bounds.into(),
                window_clip_radii: [if rounded { 12. * scale } else { 0. }; 4],
            };
            let buffer = |bytes: &[u8], usage| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytes,
                    usage,
                })
            };
            let globals_buffer = buffer(bytemuck::bytes_of(&globals), wgpu::BufferUsages::UNIFORM);
            let gamma_buffer = buffer(
                bytemuck::bytes_of(&GammaParams::zeroed()),
                wgpu::BufferUsages::UNIFORM,
            );
            let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layouts.globals,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: globals_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: gamma_buffer.as_entire_binding(),
                    },
                ],
            });
            // Two flush panels, with a seam at x=32. Their bounds are the
            // same in every case; clipping must never shift that seam.
            let left = Quad {
                bounds: Bounds {
                    size: gpui::size(ScaledPixels(20. * scale), bounds.size.height),
                    ..bounds
                },
                content_mask: gpui::ContentMask { bounds },
                background: gpui::rgb(0x204080).into(),
                ..Default::default()
            };
            let right = Quad {
                bounds: Bounds {
                    origin: gpui::point(ScaledPixels(32. * scale), bounds.origin.y),
                    size: gpui::size(ScaledPixels(52. * scale), bounds.size.height),
                },
                background: gpui::rgb(0xf0f0f0).into(),
                ..left
            };
            let quads = [left, right];
            let instances = buffer(
                unsafe { WgpuRenderer::instance_bytes(&quads) },
                wgpu::BufferUsages::STORAGE,
            );
            let instances_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layouts.instances,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: instances.as_entire_binding(),
                }],
            });
            let extent = wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            };
            let target = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let stride = (side * 4).next_multiple_of(256);
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: (stride * side) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let view = target.create_view(&Default::default());
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(&pipelines.quads);
                pass.set_bind_group(0, &globals_group, &[]);
                pass.set_bind_group(1, &instances_group, &[]);
                pass.draw(0..4, 0..2);
            }
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &target,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(stride),
                        rows_per_image: Some(side),
                    },
                },
                extent,
            );
            queue.submit([encoder.finish()]);
            readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            device.poll(wgpu::PollType::wait_indefinitely())?;
            let mapped = readback.slice(..).get_mapped_range();
            let pixels: Vec<u8> = mapped
                .chunks_exact(stride as usize)
                .flat_map(|row| row[..(side * 4) as usize].iter().copied())
                .collect();
            let pixel = |x: f32, y: f32| {
                let i = (((y * scale) as u32 * side + (x * scale) as u32) * 4) as usize;
                &pixels[i..i + 4]
            };
            assert_eq!(
                pixel(12., 48.),
                &[32, 64, 128, 255],
                "left edge must stay flush"
            );
            assert_eq!(
                pixel(31., 48.),
                &[32, 64, 128, 255],
                "sidebar width must stay fixed"
            );
            assert_eq!(
                pixel(32., 48.),
                &[240, 240, 240, 255],
                "panel seam must not move"
            );
            assert_eq!(pixel(12., 12.)[3], if rounded { 0 } else { 255 });
            assert_eq!(pixel(83., 83.)[3], if rounded { 0 } else { 255 });
            if rounded {
                assert!(
                    pixels.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255),
                    "corner must be antialiased"
                );
            }
            if let Ok(prefix) = std::env::var("TTY7_CLIP_TEST_OUTPUT") {
                std::fs::write(format!("{prefix}-{scale}-{rounded}.rgba"), &pixels)?;
            }
            drop(mapped);
            readback.unmap();
        }
    }
    Ok(())
}
