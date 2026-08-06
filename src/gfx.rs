//! Vulkan renderer (ash) — one pipeline (triangles with the atlas texture),
//! one vertex buffer re-uploaded every frame, R8 glyph atlas.

use ash::vk;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::ffi::CStr;

use crate::draw::Vertex;
use crate::font::{ATLAS_H, ATLAS_W};

const MAX_VERTS: usize = 400_000;

pub struct Gfx {
    _entry: ash::Entry,
    instance: ash::Instance,
    surface_loader: ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    pdevice: vk::PhysicalDevice,
    device: ash::Device,
    queue: vk::Queue,
    queue_family: u32,
    swapchain_loader: ash::khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    format: vk::SurfaceFormatKHR,
    pub extent: vk::Extent2D,
    images: Vec<vk::Image>,
    views: Vec<vk::ImageView>,
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    desc_layout: vk::DescriptorSetLayout,
    desc_pool: vk::DescriptorPool,
    desc_set: vk::DescriptorSet,
    sampler: vk::Sampler,
    atlas_image: vk::Image,
    atlas_mem: vk::DeviceMemory,
    atlas_view: vk::ImageView,
    atlas_initialized: bool,
    staging_buf: vk::Buffer,
    staging_mem: vk::DeviceMemory,
    staging_ptr: *mut u8,
    vertex_buf: vk::Buffer,
    vertex_mem: vk::DeviceMemory,
    vertex_ptr: *mut u8,
    cmd_pool: vk::CommandPool,
    cmd_buf: vk::CommandBuffer,
    sem_image: vk::Semaphore,
    sem_render: vk::Semaphore,
    fence: vk::Fence,
    needs_recreate: bool,
}

impl Gfx {
    pub fn new(window: &winit::window::Window) -> Self {
        unsafe {
            let entry = ash::Entry::load().expect("cannot load the Vulkan library");

            let app_name = CStr::from_bytes_with_nul(b"ng-term\0").unwrap();
            let app_info = vk::ApplicationInfo::default()
                .application_name(app_name)
                .engine_name(app_name)
                .api_version(vk::make_api_version(0, 1, 0, 0));

            let display_handle = window.display_handle().unwrap().as_raw();
            let window_handle = window.window_handle().unwrap().as_raw();

            let ext_names = ash_window::enumerate_required_extensions(display_handle)
                .expect("missing surface extensions")
                .to_vec();

            let instance_info = vk::InstanceCreateInfo::default()
                .application_info(&app_info)
                .enabled_extension_names(&ext_names);
            let instance = entry
                .create_instance(&instance_info, None)
                .expect("cannot create Vulkan instance");

            let surface = ash_window::create_surface(
                &entry,
                &instance,
                display_handle,
                window_handle,
                None,
            )
            .expect("cannot create window surface");
            let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);

            // GPU + queue family selection (graphics + present).
            let pdevices = instance
                .enumerate_physical_devices()
                .expect("no Vulkan devices");
            let (pdevice, queue_family) = pdevices
                .iter()
                .find_map(|&pd| {
                    instance
                        .get_physical_device_queue_family_properties(pd)
                        .iter()
                        .enumerate()
                        .find_map(|(i, props)| {
                            let ok = props.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                                && surface_loader
                                    .get_physical_device_surface_support(pd, i as u32, surface)
                                    .unwrap_or(false);
                            if ok { Some((pd, i as u32)) } else { None }
                        })
                })
                .expect("no GPU with graphics and present support");

            let priorities = [1.0f32];
            let queue_info = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_family)
                .queue_priorities(&priorities)];
            let dev_exts = [ash::khr::swapchain::NAME.as_ptr()];
            let device_info = vk::DeviceCreateInfo::default()
                .queue_create_infos(&queue_info)
                .enabled_extension_names(&dev_exts);
            let device = instance
                .create_device(pdevice, &device_info, None)
                .expect("cannot create logical device");
            let queue = device.get_device_queue(queue_family, 0);
            let swapchain_loader = ash::khr::swapchain::Device::new(&instance, &device);

            // Surface format: we prefer UNORM (colors written verbatim).
            let formats = surface_loader
                .get_physical_device_surface_formats(pdevice, surface)
                .unwrap();
            let format = formats
                .iter()
                .copied()
                .find(|f| {
                    f.format == vk::Format::B8G8R8A8_UNORM
                        || f.format == vk::Format::R8G8B8A8_UNORM
                })
                .unwrap_or(formats[0]);

            let render_pass = create_render_pass(&device, format.format);

            // Descriptors: atlas texture (binding 0) + sampler (binding 1),
            // because WGSL has no combined image sampler.
            let bindings = [
                vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            ];
            let desc_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .unwrap();
            let pool_sizes = [
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1),
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1),
            ];
            let desc_pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(1)
                        .pool_sizes(&pool_sizes),
                    None,
                )
                .unwrap();
            let layouts = [desc_layout];
            let desc_set = device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(desc_pool)
                        .set_layouts(&layouts),
                )
                .unwrap()[0];

            let (pipeline_layout, pipeline) =
                create_pipeline(&device, render_pass, desc_layout);

            let mem_props = instance.get_physical_device_memory_properties(pdevice);

            // Glyph atlas: R8, device-local.
            let atlas_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::R8_UNORM)
                .extent(vk::Extent3D {
                    width: ATLAS_W as u32,
                    height: ATLAS_H as u32,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            let atlas_image = device.create_image(&atlas_info, None).unwrap();
            let req = device.get_image_memory_requirements(atlas_image);
            let atlas_mem = alloc_memory(
                &device,
                &mem_props,
                req,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            );
            device.bind_image_memory(atlas_image, atlas_mem, 0).unwrap();
            let atlas_view = device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(atlas_image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(vk::Format::R8_UNORM)
                        .subresource_range(color_range()),
                    None,
                )
                .unwrap();

            let sampler = device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .unwrap();

            let tex_info = [vk::DescriptorImageInfo::default()
                .image_view(atlas_view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let samp_info = [vk::DescriptorImageInfo::default().sampler(sampler)];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(desc_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&tex_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(desc_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(&samp_info),
            ];
            device.update_descriptor_sets(&writes, &[]);

            // Staging buffer for atlas uploads.
            let (staging_buf, staging_mem, staging_ptr) = create_host_buffer(
                &device,
                &mem_props,
                (ATLAS_W * ATLAS_H) as u64,
                vk::BufferUsageFlags::TRANSFER_SRC,
            );

            // Vertex buffer (host-visible, persistently mapped).
            let (vertex_buf, vertex_mem, vertex_ptr) = create_host_buffer(
                &device,
                &mem_props,
                (MAX_VERTS * std::mem::size_of::<Vertex>()) as u64,
                vk::BufferUsageFlags::VERTEX_BUFFER,
            );

            let cmd_pool = device
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::default()
                        .queue_family_index(queue_family)
                        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                    None,
                )
                .unwrap();
            let cmd_buf = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(cmd_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .unwrap()[0];

            let sem_image = device
                .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                .unwrap();
            let sem_render = device
                .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                .unwrap();
            let fence = device
                .create_fence(
                    &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                    None,
                )
                .unwrap();

            let mut gfx = Gfx {
                _entry: entry,
                instance,
                surface_loader,
                surface,
                pdevice,
                device,
                queue,
                queue_family,
                swapchain_loader,
                swapchain: vk::SwapchainKHR::null(),
                format,
                extent: vk::Extent2D { width: 0, height: 0 },
                images: vec![],
                views: vec![],
                render_pass,
                framebuffers: vec![],
                pipeline_layout,
                pipeline,
                desc_layout,
                desc_pool,
                desc_set,
                sampler,
                atlas_image,
                atlas_mem,
                atlas_view,
                atlas_initialized: false,
                staging_buf,
                staging_mem,
                staging_ptr,
                vertex_buf,
                vertex_mem,
                vertex_ptr,
                cmd_pool,
                cmd_buf,
                sem_image,
                sem_render,
                fence,
                needs_recreate: false,
            };
            gfx.recreate_swapchain(window.inner_size().width, window.inner_size().height);
            gfx
        }
    }

    pub fn resize(&mut self) {
        self.needs_recreate = true;
    }

    fn recreate_swapchain(&mut self, width: u32, height: u32) {
        unsafe {
            let _ = self.device.device_wait_idle();
            for fb in self.framebuffers.drain(..) {
                self.device.destroy_framebuffer(fb, None);
            }
            for v in self.views.drain(..) {
                self.device.destroy_image_view(v, None);
            }

            let caps = self
                .surface_loader
                .get_physical_device_surface_capabilities(self.pdevice, self.surface)
                .unwrap();
            let extent = if caps.current_extent.width != u32::MAX {
                caps.current_extent
            } else {
                vk::Extent2D {
                    width: width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                    height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
                }
            };
            if extent.width == 0 || extent.height == 0 {
                return;
            }
            let mut image_count = caps.min_image_count + 1;
            if caps.max_image_count > 0 {
                image_count = image_count.min(caps.max_image_count);
            }
            let old = self.swapchain;
            let info = vk::SwapchainCreateInfoKHR::default()
                .surface(self.surface)
                .min_image_count(image_count)
                .image_format(self.format.format)
                .image_color_space(self.format.color_space)
                .image_extent(extent)
                .image_array_layers(1)
                .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(caps.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .present_mode(vk::PresentModeKHR::FIFO)
                .clipped(true)
                .old_swapchain(old);
            self.swapchain = self
                .swapchain_loader
                .create_swapchain(&info, None)
                .expect("cannot create swapchain");
            if old != vk::SwapchainKHR::null() {
                self.swapchain_loader.destroy_swapchain(old, None);
            }
            self.extent = extent;
            self.images = self
                .swapchain_loader
                .get_swapchain_images(self.swapchain)
                .unwrap();
            for &img in &self.images {
                let view = self
                    .device
                    .create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(img)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(self.format.format)
                            .subresource_range(color_range()),
                        None,
                    )
                    .unwrap();
                self.views.push(view);
                let attachments = [view];
                let fb = self
                    .device
                    .create_framebuffer(
                        &vk::FramebufferCreateInfo::default()
                            .render_pass(self.render_pass)
                            .attachments(&attachments)
                            .width(extent.width)
                            .height(extent.height)
                            .layers(1),
                        None,
                    )
                    .unwrap();
                self.framebuffers.push(fb);
            }
            self.needs_recreate = false;
        }
    }

    /// Renders a frame. Pass `atlas` only when the atlas has changed.
    pub fn render(
        &mut self,
        window: &winit::window::Window,
        verts: &[Vertex],
        atlas: Option<&[u8]>,
        clear: [f32; 4],
    ) {
        unsafe {
            if self.needs_recreate {
                let size = window.inner_size();
                self.recreate_swapchain(size.width, size.height);
            }
            if self.extent.width == 0 || self.extent.height == 0 {
                return;
            }

            self.device
                .wait_for_fences(&[self.fence], true, u64::MAX)
                .unwrap();

            let acquired = self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                self.sem_image,
                vk::Fence::null(),
            );
            let image_index = match acquired {
                Ok((idx, _)) => idx,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.needs_recreate = true;
                    return;
                }
                Err(e) => panic!("acquire_next_image: {e:?}"),
            };

            self.device.reset_fences(&[self.fence]).unwrap();

            // Upload vertices (the buffer is persistently mapped).
            let n = verts.len().min(MAX_VERTS);
            std::ptr::copy_nonoverlapping(
                verts.as_ptr() as *const u8,
                self.vertex_ptr,
                n * std::mem::size_of::<Vertex>(),
            );

            let upload_atlas = if let Some(data) = atlas {
                std::ptr::copy_nonoverlapping(data.as_ptr(), self.staging_ptr, data.len());
                true
            } else {
                !self.atlas_initialized
            };

            let cmd = self.cmd_buf;
            self.device
                .reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
                .unwrap();
            self.device
                .begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .unwrap();

            if upload_atlas {
                self.record_atlas_upload(cmd);
                self.atlas_initialized = true;
            }

            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue { float32: clear },
            }];
            let rp_info = vk::RenderPassBeginInfo::default()
                .render_pass(self.render_pass)
                .framebuffer(self.framebuffers[image_index as usize])
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: self.extent,
                })
                .clear_values(&clear_values);
            self.device
                .cmd_begin_render_pass(cmd, &rp_info, vk::SubpassContents::INLINE);

            self.device
                .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            let viewport = vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: self.extent.width as f32,
                height: self.extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            };
            self.device.cmd_set_viewport(cmd, 0, &[viewport]);
            self.device.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: self.extent,
                }],
            );
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.desc_set],
                &[],
            );
            self.device
                .cmd_bind_vertex_buffers(cmd, 0, &[self.vertex_buf], &[0]);
            let screen = [self.extent.width as f32, self.extent.height as f32];
            self.device.cmd_push_constants(
                cmd,
                self.pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                std::slice::from_raw_parts(screen.as_ptr() as *const u8, 8),
            );
            if n > 0 {
                self.device.cmd_draw(cmd, n as u32, 1, 0, 0);
            }
            self.device.cmd_end_render_pass(cmd);
            self.device.end_command_buffer(cmd).unwrap();

            let wait_sems = [self.sem_image];
            let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let signal_sems = [self.sem_render];
            let cmds = [cmd];
            let submit = vk::SubmitInfo::default()
                .wait_semaphores(&wait_sems)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(&cmds)
                .signal_semaphores(&signal_sems);
            self.device
                .queue_submit(self.queue, &[submit], self.fence)
                .unwrap();

            let swapchains = [self.swapchain];
            let indices = [image_index];
            let present = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal_sems)
                .swapchains(&swapchains)
                .image_indices(&indices);
            match self.swapchain_loader.queue_present(self.queue, &present) {
                Ok(suboptimal) => {
                    if suboptimal {
                        self.needs_recreate = true;
                    }
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => self.needs_recreate = true,
                Err(e) => panic!("queue_present: {e:?}"),
            }
        }
    }

    unsafe fn record_atlas_upload(&self, cmd: vk::CommandBuffer) {
        let to_dst = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(if self.atlas_initialized {
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
            } else {
                vk::ImageLayout::UNDEFINED
            })
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.atlas_image)
            .subresource_range(color_range());
        self.device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_dst],
        );
        let region = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .mip_level(0)
                    .base_array_layer(0)
                    .layer_count(1),
            )
            .image_extent(vk::Extent3D {
                width: ATLAS_W as u32,
                height: ATLAS_H as u32,
                depth: 1,
            });
        self.device.cmd_copy_buffer_to_image(
            cmd,
            self.staging_buf,
            self.atlas_image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[region],
        );
        let to_read = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.atlas_image)
            .subresource_range(color_range());
        self.device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_read],
        );
    }
}

impl Drop for Gfx {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            let d = &self.device;
            d.destroy_fence(self.fence, None);
            d.destroy_semaphore(self.sem_image, None);
            d.destroy_semaphore(self.sem_render, None);
            d.destroy_command_pool(self.cmd_pool, None);
            d.destroy_buffer(self.vertex_buf, None);
            d.free_memory(self.vertex_mem, None);
            d.destroy_buffer(self.staging_buf, None);
            d.free_memory(self.staging_mem, None);
            d.destroy_sampler(self.sampler, None);
            d.destroy_image_view(self.atlas_view, None);
            d.destroy_image(self.atlas_image, None);
            d.free_memory(self.atlas_mem, None);
            d.destroy_descriptor_pool(self.desc_pool, None);
            d.destroy_descriptor_set_layout(self.desc_layout, None);
            d.destroy_pipeline(self.pipeline, None);
            d.destroy_pipeline_layout(self.pipeline_layout, None);
            for fb in self.framebuffers.drain(..) {
                d.destroy_framebuffer(fb, None);
            }
            for v in self.views.drain(..) {
                d.destroy_image_view(v, None);
            }
            d.destroy_render_pass(self.render_pass, None);
            if self.swapchain != vk::SwapchainKHR::null() {
                self.swapchain_loader.destroy_swapchain(self.swapchain, None);
            }
            d.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
            let _ = self.queue_family;
        }
    }
}

fn color_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
}

fn create_render_pass(device: &ash::Device, format: vk::Format) -> vk::RenderPass {
    let attachments = [vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];
    let color_refs = [vk::AttachmentReference::default()
        .attachment(0)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
    let subpasses = [vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_refs)];
    let deps = [vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags::empty())
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)];
    unsafe {
        device
            .create_render_pass(
                &vk::RenderPassCreateInfo::default()
                    .attachments(&attachments)
                    .subpasses(&subpasses)
                    .dependencies(&deps),
                None,
            )
            .unwrap()
    }
}

fn create_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    desc_layout: vk::DescriptorSetLayout,
) -> (vk::PipelineLayout, vk::Pipeline) {
    unsafe {
        // One SPIR-V module with two entry points (vs_main / fs_main).
        let spv = crate::shaders::compile();
        let shader_mod = device
            .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&spv), None)
            .unwrap();

        let vs_entry = CStr::from_bytes_with_nul(b"vs_main\0").unwrap();
        let fs_entry = CStr::from_bytes_with_nul(b"fs_main\0").unwrap();
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(shader_mod)
                .name(vs_entry),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(shader_mod)
                .name(fs_entry),
        ];

        let bindings = [vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(std::mem::size_of::<Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)];
        let attrs = [
            vk::VertexInputAttributeDescription::default()
                .location(0)
                .binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(0),
            vk::VertexInputAttributeDescription::default()
                .location(1)
                .binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(8),
            vk::VertexInputAttributeDescription::default()
                .location(2)
                .binding(0)
                .format(vk::Format::R32G32B32A32_SFLOAT)
                .offset(16),
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&bindings)
            .vertex_attribute_descriptions(&attrs);

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::CLOCKWISE)
            .line_width(1.0);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(vk::ColorComponentFlags::RGBA)];
        let blend =
            vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let push_ranges = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(8)];
        let set_layouts = [desc_layout];
        let layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&set_layouts)
                    .push_constant_ranges(&push_ranges),
                None,
            )
            .unwrap();

        let info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&raster)
            .multisample_state(&multisample)
            .color_blend_state(&blend)
            .dynamic_state(&dynamic)
            .layout(layout)
            .render_pass(render_pass)
            .subpass(0);
        let pipeline = device
            .create_graphics_pipelines(vk::PipelineCache::null(), &[info], None)
            .expect("cannot create pipeline")[0];

        device.destroy_shader_module(shader_mod, None);
        (layout, pipeline)
    }
}

fn find_memory_type(
    props: &vk::PhysicalDeviceMemoryProperties,
    type_bits: u32,
    flags: vk::MemoryPropertyFlags,
) -> u32 {
    for i in 0..props.memory_type_count {
        if type_bits & (1 << i) != 0
            && props.memory_types[i as usize].property_flags.contains(flags)
        {
            return i;
        }
    }
    panic!("no suitable GPU memory type");
}

fn alloc_memory(
    device: &ash::Device,
    props: &vk::PhysicalDeviceMemoryProperties,
    req: vk::MemoryRequirements,
    flags: vk::MemoryPropertyFlags,
) -> vk::DeviceMemory {
    unsafe {
        device
            .allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(req.size)
                    .memory_type_index(find_memory_type(props, req.memory_type_bits, flags)),
                None,
            )
            .unwrap()
    }
}

fn create_host_buffer(
    device: &ash::Device,
    props: &vk::PhysicalDeviceMemoryProperties,
    size: u64,
    usage: vk::BufferUsageFlags,
) -> (vk::Buffer, vk::DeviceMemory, *mut u8) {
    unsafe {
        let buf = device
            .create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(size)
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )
            .unwrap();
        let req = device.get_buffer_memory_requirements(buf);
        let mem = alloc_memory(
            device,
            props,
            req,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        );
        device.bind_buffer_memory(buf, mem, 0).unwrap();
        let ptr = device
            .map_memory(mem, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())
            .unwrap() as *mut u8;
        (buf, mem, ptr)
    }
}
