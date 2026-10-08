#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::main_pass_post_lighting_processing,
}
#ifdef DEPTH_PREPASS
#import bevy_pbr::{prepass_utils::prepass_depth, view_transformations::depth_ndc_to_view_z}
#endif

@group(2) @binding(100) var<uniform> fade_distance: vec4<f32>;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var input = pbr_input_from_standard_material(in, is_front);
#ifdef DEPTH_PREPASS
    let opaque_depth = prepass_depth(in.position, 0u);
    if opaque_depth > 0.0 {
        let surface = depth_ndc_to_view_z(opaque_depth);
        let particle = depth_ndc_to_view_z(in.position.z);
        let gap = particle - surface;
        input.material.base_color.a *= smoothstep(0.0, max(fade_distance.x, 0.0001), gap);
    }
#endif
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(input, input.material.base_color);
    return out;
}
