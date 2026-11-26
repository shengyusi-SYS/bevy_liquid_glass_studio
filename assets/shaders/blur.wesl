// Match the original Gaussian convolution in encoded sRGB values. Public
// source/blurred textures retain Bevy's linear sampling contract; only the
// prefilter and vertical intermediates store the reference working values.
import bevy_render::color_operations::{linear_rgb_to_srgb, srgb_to_linear_rgb};

struct BlurUniform {
    step: vec2<f32>,
    tap_count: u32,
    // 0: raw linear input; 1: working values; 2: paired working values.
    sampling: u32,
    kernel: array<vec4<f32>, 51>,
};
@group(0) @binding(0) var source_image: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> settings: BlurUniform;

fn kernel(index: u32) -> f32 {
    return settings.kernel[index / 4u][index % 4u];
}

fn sample_working(uv: vec2<f32>) -> vec4<f32> {
    let color = textureSampleLevel(source_image, source_sampler, uv, 0.0);
    if settings.sampling == 0u {
        return vec4<f32>(linear_rgb_to_srgb(color.rgb), color.a);
    }
    return color;
}

fn load_working(pixel: vec2<i32>, size: vec2<i32>) -> vec4<f32> {
    let color = textureLoad(source_image, clamp(pixel, vec2<i32>(0), size - 1), 0);
    return vec4<f32>(linear_rgb_to_srgb(color.rgb), color.a);
}

// Filter in the reference working space before decimation. Sampling the sRGB
// texture with a linear sampler first would brighten high-frequency details.
@fragment
fn downsample(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(source_image));
    let position = uv * vec2<f32>(size) - 0.5;
    let pixel = vec2<i32>(floor(position));
    let fraction = fract(position);
    return mix(
        mix(load_working(pixel, size), load_working(pixel + vec2<i32>(1, 0), size), fraction.x),
        mix(load_working(pixel + vec2<i32>(0, 1), size), load_working(pixel + vec2<i32>(1, 1), size), fraction.x),
        fraction.y
    );
}

@fragment
fn fragment(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    var color = sample_working(uv) * kernel(0u);
    if settings.sampling == 2u {
        // Half's prefilter already converted every texel to working values,
        // so interpolation can combine adjacent Gaussian taps in that space.
        for (var i = 0u; i < settings.tap_count; i += 1u) {
            let weight = kernel(1u + i * 2u);
            let offset = settings.step * kernel(2u + i * 2u);
            color += (sample_working(uv + offset) + sample_working(uv - offset)) * weight;
        }
    } else {
        // Preserve Full's reference convolution, including its nonlinear
        // conversion of each raw vertical sample before weighting.
        for (var i = 1u; i <= settings.tap_count; i += 1u) {
            let offset = settings.step * f32(i);
            color += (sample_working(uv + offset) + sample_working(uv - offset)) * kernel(i);
        }
    }
    if settings.step.x > 0.0 {
        return vec4<f32>(srgb_to_linear_rgb(color.rgb), color.a);
    }
    return color;
}
