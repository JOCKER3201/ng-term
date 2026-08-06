//! WGSL -> SPIR-V compilation at startup using naga (pure Rust,
//! no external tools like glslc). One module with two entry points.

pub const WGSL_SRC: &str = r#"
struct Push {
    screen: vec2<f32>,
};

var<push_constant> pc: Push;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) a_pos: vec2<f32>,
    @location(1) a_uv: vec2<f32>,
    @location(2) a_color: vec4<f32>,
) -> VsOut {
    var out: VsOut;
    let ndc = a_pos / pc.screen * 2.0 - vec2<f32>(1.0, 1.0);
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = a_uv;
    out.color = a_color;
    return out;
}

@group(0) @binding(0) var t_atlas: texture_2d<f32>;
@group(0) @binding(1) var s_atlas: sampler;

@fragment
fn fs_main(
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
) -> @location(0) vec4<f32> {
    let coverage = textureSample(t_atlas, s_atlas, uv).r;
    return vec4<f32>(color.rgb, color.a * coverage);
}
"#;

pub fn compile() -> Vec<u32> {
    let module = naga::front::wgsl::parse_str(WGSL_SRC)
        .unwrap_or_else(|e| panic!("WGSL compilation error: {e}"));

    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("shader validation error: {e:?}"));

    // The shader computes NDC in Vulkan convention (Y down), so we disable
    // the default WebGPU->Vulkan coordinate-space conversion (Y flip).
    let mut options = naga::back::spv::Options::default();
    options
        .flags
        .remove(naga::back::spv::WriterFlags::ADJUST_COORDINATE_SPACE);

    naga::back::spv::write_vec(&module, &info, &options, None)
        .unwrap_or_else(|e| panic!("SPIR-V write error: {e:?}"))
}
