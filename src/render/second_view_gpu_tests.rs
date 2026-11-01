//! Concurrent projections share immutable textures and keep mutable GPU state local.

use super::batch::{BatchRenderer, SpriteInstance};
use super::tactical_draw_plan::RenderZPolicy;
use super::terrain_draw::{DestinationEditCommand, TerrainDrawRenderer, TerrainPiece};
use super::terrain_draw_gpu_tests::{Gpu, camera, clear, encoded, sprite};
use wgpu::util::DeviceExt;

const SIZE: [u32; 2] = [8, 4];
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

fn landmarks() -> [SpriteInstance; 2] {
    [
        SpriteInstance {
            uv_size: [0.5, 1.0],
            ..sprite([0.0, 0.0], [2.0, 2.0], -1.0)
        },
        SpriteInstance {
            uv_origin: [0.5, 0.0],
            uv_size: [0.5, 1.0],
            ..sprite([100.0, 0.0], [2.0, 2.0], -1.0)
        },
    ]
}

fn landmark_atlas(gpu: &Gpu, batch: &BatchRenderer) -> super::batch::BatchTexture {
    batch.create_texture_on_device(
        &gpu.device,
        &gpu.queue,
        &[255, 0, 0, 255, 0, 255, 0, 255],
        2,
        1,
        Some(&[1, 2]),
    )
}

fn buffer(gpu: &Gpu, instances: &[SpriteInstance]) -> wgpu::Buffer {
    gpu.device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Two-view landmarks"),
            contents: bytemuck::cast_slice(instances),
            usage: wgpu::BufferUsages::VERTEX,
        })
}

fn second_camera() -> super::batch::CameraUniform {
    super::batch::CameraUniform {
        camera_pos: [100.0, 0.0],
        zoom: 2.0,
        ..camera(SIZE)
    }
}

fn pixel(bytes: &[u8], x: usize, y: usize) -> &[u8] {
    let offset = (y * SIZE[0] as usize + x) * 4;
    &bytes[offset..offset + 4]
}

#[test]
#[ignore = "requires GPU; two production Batch projections in one pending submission"]
fn forked_batch_cameras_render_distinct_landmarks_without_overwriting_pending_draws() {
    let gpu = Gpu::new();
    let main = BatchRenderer::new_with_device(&gpu.device, &gpu.queue, FORMAT);
    main.write_camera(&gpu.queue, camera(SIZE));
    let second = main.fork_view_with_device(&gpu.device);
    let atlas = landmark_atlas(&gpu, &main);
    let instances = buffer(&gpu, &landmarks());
    let colors = [gpu.target(SIZE, FORMAT), gpu.target(SIZE, FORMAT)];
    let views = colors
        .each_ref()
        .map(|color| color.create_view(&Default::default()));
    let depths = [
        gpu.target(SIZE, wgpu::TextureFormat::Depth32Float),
        gpu.target(SIZE, wgpu::TextureFormat::Depth32Float),
    ];
    let depth_views = depths
        .each_ref()
        .map(|depth| depth.create_view(&Default::default()));
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    for (index, batch) in [&main, &second].into_iter().enumerate() {
        if index == 1 {
            second.write_camera(&gpu.queue, second_camera());
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &views[index],
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_views[index],
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        batch.draw_with_buffer_passthrough(&mut pass, &atlas, &instances, 2);
    }
    let reads = colors.each_ref().map(|color| gpu.read(&mut encoder, color));
    let output = gpu.finish(encoder, &reads, SIZE);
    assert_eq!(pixel(&output[0], 1, 1), [255, 0, 0, 255]);
    assert_eq!(pixel(&output[0], 3, 3), [0, 0, 0, 255]);
    assert_eq!(pixel(&output[1], 1, 1), [0, 255, 0, 255]);
    assert_eq!(pixel(&output[1], 3, 3), [0, 255, 0, 255]);
    assert_eq!(main.camera_uniform().camera_pos, [0.0, 0.0]);
    assert_eq!(main.camera_uniform().zoom, 1.0);
}

#[test]
#[ignore = "requires GPU; interleaved production destination edits retain per-view camera and scratch targets"]
fn forked_terrain_targets_keep_interleaved_destination_edits_in_their_own_view() {
    let gpu = Gpu::new();
    let main_batch = BatchRenderer::new_with_device(&gpu.device, &gpu.queue, FORMAT);
    main_batch.write_camera(&gpu.queue, camera(SIZE));
    let second_batch = main_batch.fork_view_with_device(&gpu.device);
    let mut main = TerrainDrawRenderer::new(&gpu.device, &gpu.queue, FORMAT, &main_batch);
    let mut second = main.fork_view(&gpu.device, &gpu.queue, &second_batch);
    let atlas = landmark_atlas(&gpu, &main_batch);
    let landmarks = landmarks();
    let instances = [
        landmarks[0],
        landmarks[1],
        SpriteInstance {
            z_adjust: -2.0,
            ..landmarks[0]
        },
    ];
    let vertices = buffer(&gpu, &instances);
    let colors = [gpu.target(SIZE, FORMAT), gpu.target(SIZE, FORMAT)];
    let views = colors
        .each_ref()
        .map(|color| color.create_view(&Default::default()));
    let depths = [
        gpu.target(SIZE, wgpu::TextureFormat::Depth32Float),
        gpu.target(SIZE, wgpu::TextureFormat::Depth32Float),
    ];
    let depth_views = depths
        .each_ref()
        .map(|depth| depth.create_view(&Default::default()));
    main.prepare(
        &gpu.device,
        &colors[0],
        &depth_views[0],
        main_batch.camera_uniform(),
    );
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    for index in 0..2 {
        clear(
            &mut encoder,
            &views[index],
            &depth_views[index],
            65535,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        );
    }
    let draws = [0, 1].map(|index| DestinationEditCommand {
        index,
        piece: TerrainPiece::Body,
        render_z: RenderZPolicy::ReadWrite,
        atlas_slot: 0,
    });
    main.draw_span(
        &mut encoder,
        &views[0],
        &depth_views[0],
        &main_batch,
        |_| Some(&atlas),
        &vertices,
        &instances,
        draws,
        [0, 0, SIZE[0], SIZE[1]],
    );
    second_batch.write_camera(&gpu.queue, second_camera());
    second.prepare(
        &gpu.device,
        &colors[1],
        &depth_views[1],
        second_batch.camera_uniform(),
    );
    second.draw_span(
        &mut encoder,
        &views[1],
        &depth_views[1],
        &second_batch,
        |_| Some(&atlas),
        &vertices,
        &instances,
        draws,
        [0, 0, SIZE[0], SIZE[1]],
    );
    main.draw_span(
        &mut encoder,
        &views[0],
        &depth_views[0],
        &main_batch,
        |_| Some(&atlas),
        &vertices,
        &instances,
        [DestinationEditCommand {
            index: 2,
            piece: TerrainPiece::Shadow,
            render_z: RenderZPolicy::ReadWrite,
            atlas_slot: 0,
        }],
        [0, 0, SIZE[0], SIZE[1]],
    );
    let reads = colors.each_ref().map(|color| gpu.read(&mut encoder, color));
    let output = gpu.finish(encoder, &reads, SIZE);
    let red = pixel(&output[0], 1, 1);
    assert!(
        red[0] > 0 && red[0] < encoded(0xf800, FORMAT)[0],
        "main pixel={red:?}, full red={:?}, second pixel={:?}",
        encoded(0xf800, FORMAT),
        pixel(&output[1], 1, 1)
    );
    assert_eq!(&red[1..], [0, 0, 255]);
    assert_eq!(pixel(&output[0], 3, 3), [0, 0, 0, 255]);
    assert_eq!(pixel(&output[1], 1, 1), encoded(0x07e0, FORMAT));
    assert_eq!(pixel(&output[1], 3, 3), encoded(0x07e0, FORMAT));
}
