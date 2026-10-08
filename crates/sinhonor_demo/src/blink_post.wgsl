#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct BlinkLens {
    blur: f32,
    distortion: f32,
    time: f32,
    aspect: f32,
}

@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;
@group(0) @binding(2) var<uniform> lens: BlinkLens;

fn sample_scene(uv: vec2<f32>) -> vec4<f32> {
    let inset = 0.5 / vec2<f32>(textureDimensions(scene));
    return textureSampleLevel(scene, scene_sampler, clamp(uv, inset, vec2<f32>(1.0) - inset), 0.0);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let delta = in.uv - vec2<f32>(0.5);
    let radius = length(delta * vec2<f32>(max(lens.aspect, 0.1), 1.0));
    // Protect the aiming point while distortion and streaks build toward the periphery.
    let edge = smoothstep(0.12, 0.85, radius);
    let blur = clamp(lens.blur, 0.0, 1.0);
    let distortion = clamp(lens.distortion, 0.0, 1.0);
    let wave = sin(radius * 15.0 - lens.time * 9.0);
    let uv = in.uv - delta * edge * distortion * (0.025 + 0.012 * wave);
    let streak = delta * edge * blur * 0.24;
    let center = sample_scene(uv).rgb;
    var color = center;
    // A weighted radial gather streaks the scene without relying on motion vectors.
    if blur > 0.0001 {
        var total = 1.0;
        for (var i = 1; i < 12; i += 1) {
            let t = f32(i) / 11.0;
            let weight = 1.0 - 0.65 * t;
            color += sample_scene(uv - streak * t).rgb * weight;
            total += weight;
        }
        color /= total;
    }
    // Restrained spectral fringe, strongest on release; never shifts the centre reticle.
    let fringe = delta * edge * (distortion * 0.004 + blur * 0.006);
    color += vec3<f32>(sample_scene(uv + fringe).r - center.r, 0.0,
        sample_scene(uv - fringe).b - center.b) * 0.6;
    let tunnel = smoothstep(0.25, 1.0, length(delta * vec2<f32>(1.6, 2.0)));
    color *= 1.0 - tunnel * clamp(blur * 0.72 + distortion * 0.18, 0.0, 0.8);
    return vec4<f32>(max(color, vec3<f32>(0.0)), sample_scene(in.uv).a);
}
