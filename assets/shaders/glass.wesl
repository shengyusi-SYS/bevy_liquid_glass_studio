// Adapted from iyinchao/liquid-glass-studio, MIT, Copyright (c) 2024 Charles Yin.
// The edge-refraction, RGB dispersion, Fresnel, glare and Lab/LCH model are
// retained, while shape coordinates and strength are local to a Bevy UI node.
// Samples arrive as linear RGB. The reference optical model mixes sRGB working
// values and decodes its result once; final target encoding follows the camera.
import bevy_ui_render::ui_vertex_output::UiVertexOutput;
import bevy_render::writer_encode::writer_encode;
import bevy_render::color_operations::{linear_rgb_to_srgb, srgb_to_linear_rgb};

struct GlassUniform {
    target_size: vec2<f32>,
    scale_factor: f32,
    corner_radius: f32,
    roundness: f32,
    refraction_thickness: f32,
    refraction_distance: f32,
    refractive_index: f32,
    dispersion: f32,
    blur_edge: u32,
    fresnel_range: f32,
    fresnel_hardness: f32,
    fresnel_strength: f32,
    glare_angle: f32,
    glare_range: f32,
    glare_hardness: f32,
    glare_strength: f32,
    glare_convergence: f32,
    glare_opposite_strength: f32,
    lighting_gain: f32,
    edge_feather: f32,
    tint: vec4<f32>,
};

@group(1) @binding(0) var<uniform> glass: GlassUniform;
@group(1) @binding(1) var source_image: texture_2d<f32>;
@group(1) @binding(2) var source_sampler: sampler;
@group(1) @binding(3) var blurred_image: texture_2d<f32>;
@group(1) @binding(4) var blurred_sampler: sampler;


struct GlassGroupShape {
    center: vec2<f32>,
    half_size: vec2<f32>,
    corner_radius: f32,
    roundness: f32,
    _padding: vec2<f32>,
};

struct GlassGroupUniform {
    shape_count: u32,
    blend_radius: f32,
    shadow_blur: f32,
    shadow_spread: f32,
    shadow_offset: vec2<f32>,
    _padding: vec2<f32>,
    shadow_color: vec4<f32>,
    shapes: array<GlassGroupShape, 64>,
};

// Only fragment_group reaches this binding. Individual glass nodes use the
// same optics and shader asset without needing a group uniform binding.
@group(1) @binding(5) var<uniform> group: GlassGroupUniform;

const PI: f32 = 3.14159265359;
// Original getNormal multiplies the normalized SDF gradient by sqrt(2)*1000.
const REFERENCE_NORMAL_SCALE: f32 = 1414.213562;
const D65_WHITE: vec3<f32> = vec3<f32>(0.95045592705, 1.0, 1.08905775076);

fn linear_to_xyz(rgb: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        dot(rgb, vec3<f32>(0.4124, 0.3576, 0.1805)),
        dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722)),
        dot(rgb, vec3<f32>(0.0193, 0.1192, 0.9505))
    );
}

fn xyz_to_linear(xyz: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        dot(xyz, vec3<f32>(3.2406255, -1.537208, -0.4986286)),
        dot(xyz, vec3<f32>(-0.9689307, 1.8757561, 0.0415175)),
        dot(xyz, vec3<f32>(0.0557101, -0.2040211, 1.0569959))
    );
}

fn lab_f(x: f32) -> f32 {
    if x > 0.00885645167 {
        return pow(x, 1.0 / 3.0);
    }
    return 7.78703703704 * x + 0.13793103448;
}

fn lab_f_inverse(x: f32) -> f32 {
    if x > 0.206897 {
        return x * x * x;
    }
    return 0.12841854934 * (x - 0.13793103448);
}

fn linear_to_lch(rgb: vec3<f32>) -> vec3<f32> {
    let xyz = linear_to_xyz(rgb) / D65_WHITE;
    let v = vec3<f32>(lab_f(xyz.x), lab_f(xyz.y), lab_f(xyz.z));
    let lab = vec3<f32>(116.0 * v.y - 16.0, 500.0 * (v.x - v.y), 200.0 * (v.y - v.z));
    return vec3<f32>(lab.x, length(lab.yz), atan2(lab.z, lab.y));
}

fn lch_to_linear(lch: vec3<f32>) -> vec3<f32> {
    let lab = vec3<f32>(lch.x, lch.y * cos(lch.z), lch.y * sin(lch.z));
    let w = (lab.x + 16.0) / 116.0;
    let xyz = D65_WHITE * vec3<f32>(
        lab_f_inverse(w + lab.y / 500.0),
        lab_f_inverse(w),
        lab_f_inverse(w - lab.z / 200.0)
    );
    return xyz_to_linear(xyz);
}

fn rounded_box_distance(p: vec2<f32>, half_size: vec2<f32>, corner_radius: f32, roundness: f32) -> f32 {
    let radius = min(max(corner_radius, 0.0), min(half_size.x, half_size.y));
    let q = abs(p) - half_size + vec2<f32>(radius);
    if radius <= 0.001 {
        return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0)));
    }
    // Outside the corner quadrant, the superellipse norm has at most one
    // nonzero component and the full distance reduces to the nearest side.
    if min(q.x, q.y) <= 0.0 {
        return max(q.x, q.y) - radius;
    }
    let exponent = max(roundness, 2.0);
    if exponent == 2.0 {
        return length(q) - radius;
    }
    // Normalize before pow(), keeping the superellipse stable at UI sizes.
    let corner = q / radius;
    let curved = pow(pow(corner.x, exponent) + pow(corner.y, exponent), 1.0 / exponent) * radius;
    return curved - radius;
}

fn srgb_to_lch(color: vec3<f32>) -> vec3<f32> {
    return linear_to_lch(srgb_to_linear_rgb(color));
}

fn lch_to_srgb(color: vec3<f32>) -> vec3<f32> {
    return linear_rgb_to_srgb(lch_to_linear(color));
}

fn shape_distance(p: vec2<f32>, size: vec2<f32>) -> f32 {
    return rounded_box_distance(p, size * 0.5, glass.corner_radius * glass.scale_factor, glass.roundness);
}

fn shape_gradient(p: vec2<f32>, size: vec2<f32>) -> vec2<f32> {
    // Keep the SDF derivative's magnitude, as in the original model. Normalizing
    // it would give superellipse corners the same lighting as straight sides.
    return 0.5 * vec2<f32>(
        shape_distance(p + vec2<f32>(1.0, 0.0), size) - shape_distance(p - vec2<f32>(1.0, 0.0), size),
        shape_distance(p + vec2<f32>(0.0, 1.0), size) - shape_distance(p - vec2<f32>(0.0, 1.0), size)
    );
}

fn smooth_union(a: f32, b: f32, radius: f32) -> f32 {
    if radius <= 0.0 {
        return min(a, b);
    }
    let h = clamp(0.5 + 0.5 * (b - a) / radius, 0.0, 1.0);
    return mix(b, a, h) - radius * h * (1.0 - h);
}

fn group_distance(p: vec2<f32>) -> f32 {
    var distance = 1000000.0;
    for (var i = 0u; i < min(group.shape_count, 64u); i += 1u) {
        let shape = group.shapes[i];
        let local = p - shape.center;
        // The enclosing box's L-infinity distance is a lower bound for the
        // rounded/superellipse distance. Beyond the blend support this member
        // cannot change the current union, so avoid its expensive pow() calls.
        let box_distance = abs(local) - shape.half_size;
        if max(box_distance.x, box_distance.y) >= distance + max(group.blend_radius, 0.0) {
            continue;
        }
        let member = rounded_box_distance(local, shape.half_size, shape.corner_radius, shape.roundness);
        distance = smooth_union(distance, member, group.blend_radius);
    }
    return distance;
}

fn group_gradient(p: vec2<f32>) -> vec2<f32> {
    return 0.5 * vec2<f32>(
        group_distance(p + vec2<f32>(1.0, 0.0)) - group_distance(p - vec2<f32>(1.0, 0.0)),
        group_distance(p + vec2<f32>(0.0, 1.0)) - group_distance(p - vec2<f32>(0.0, 1.0))
    );
}

fn sample_source(uv: vec2<f32>) -> vec3<f32> {
    return linear_rgb_to_srgb(textureSampleLevel(source_image, source_sampler, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb);
}

fn sample_blurred(uv: vec2<f32>) -> vec3<f32> {
    return linear_rgb_to_srgb(textureSampleLevel(blurred_image, blurred_sampler, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb);
}

fn dispersed_sample(uv: vec2<f32>, offset: vec2<f32>, blur_mix: f32) -> vec3<f32> {
    let red_uv = uv + offset * (1.0 + 0.02 * glass.dispersion);
    let green_uv = uv + offset;
    let blue_uv = uv + offset * (1.0 - 0.02 * glass.dispersion);
    // Uniform no-dispersion and fully blurred edges need fewer texture reads.
    if glass.dispersion == 0.0 {
        if blur_mix >= 1.0 { return sample_blurred(green_uv); }
        if blur_mix <= 0.0 { return sample_source(green_uv); }
        return mix(sample_source(green_uv), sample_blurred(green_uv), blur_mix);
    }
    if blur_mix >= 1.0 {
        return vec3<f32>(sample_blurred(red_uv).r, sample_blurred(green_uv).g, sample_blurred(blue_uv).b);
    }
    let raw = vec3<f32>(sample_source(red_uv).r, sample_source(green_uv).g, sample_source(blue_uv).b);
    if blur_mix <= 0.0 { return raw; }
    let blurred = vec3<f32>(sample_blurred(red_uv).r, sample_blurred(green_uv).g, sample_blurred(blue_uv).b);
    return mix(raw, blurred, blur_mix);
}

fn edge_rim(depth: f32, range: f32, hardness: f32) -> f32 {
    if range <= 0.0 {
        return 0.0;
    }
    // Preserve the original STEP9 range mapping. With range=30/hardness=.2,
    // the full-intensity plateau is ~1 logical pixel, not a 6px padded frame.
    let logical_depth = depth / glass.scale_factor;
    let base = 1.0 - logical_depth / 1500.0 * pow(500.0 / range, 2.0) + hardness;
    return pow(clamp(base, 0.0, 1.0), 5.0);
}

fn coverage_at(distance: f32) -> f32 {
    // Stable uses a logical-pixel feather; OriginalCanvas keeps .001*Hphysical.
    let feather = glass.edge_feather;
    return 1.0 - smoothstep(-feather, feather, distance);
}

fn needs_gradient(distance: f32) -> bool {
    return glass.refraction_thickness > 0.0 && -distance < glass.refraction_thickness * glass.scale_factor;
}

fn encode_color(color: vec3<f32>) -> vec3<f32> {
    return writer_encode(vec4<f32>(color, 1.0)).rgb;
}

fn shade_glass(position: vec2<f32>, distance: f32, gradient: vec2<f32>, coverage: f32) -> vec4<f32> {
    if coverage <= 0.0 {
        return vec4<f32>(0.0);
    }
    let uv = position / max(glass.target_size, vec2<f32>(1.0));
    let depth = max(-distance, 0.0);
    let thickness = max(glass.refraction_thickness * glass.scale_factor, 0.001);
    var refraction = 0.0;
    if glass.refraction_thickness > 0.0 && depth < thickness {
        let incidence = asin(pow(clamp(1.0 - depth / thickness, 0.0, 1.0), 2.0));
        let transmission = asin(sin(incidence) / max(glass.refractive_index, 1.0));
        refraction = tan(incidence - transmission);
    }

    let tint = vec4<f32>(linear_rgb_to_srgb(glass.tint.rgb), glass.tint.a);
    var color: vec3<f32>;
    if refraction > 0.0 {
        // source getNormal = local_gradient * sqrt(2)*1000 / Hphysical.
        // Its DPR and aspect correction cancel Hphysical for displacement:
        // logical shift = local_gradient * edge_factor * distance * sqrt(2)*1000.
        let normal_length = length(gradient) * glass.lighting_gain;
        let offset = -gradient * refraction * glass.refraction_distance * REFERENCE_NORMAL_SCALE * glass.scale_factor / max(glass.target_size, vec2<f32>(1.0));
        var blur_mix = clamp(depth / thickness, 0.0, 1.0);
        if glass.blur_edge != 0u {
            blur_mix = 1.0;
        }
        let refracted = dispersed_sample(uv, offset, blur_mix);
        color = mix(refracted, tint.rgb, tint.a * 0.8);

        if glass.fresnel_strength != 0.0 {
            let fresnel = edge_rim(depth, glass.fresnel_range, glass.fresnel_hardness);
            var fresnel_lch = srgb_to_lch(mix(vec3<f32>(1.0), tint.rgb, tint.a * 0.5));
            fresnel_lch.x = clamp(fresnel_lch.x + 20.0 * fresnel * glass.fresnel_strength, 0.0, 100.0);
            color = mix(color, lch_to_srgb(fresnel_lch), fresnel * glass.fresnel_strength * 0.7 * normal_length);
        }

        if glass.glare_strength != 0.0 {
            let glare_geometry = edge_rim(depth, glass.glare_range, glass.glare_hardness);
            // atan2 keeps the original angle convention; the raw gradient supplies
            // lighting intensity and refraction, while angle uses only direction.
            var normal_angle = 0.0;
            if normal_length > 0.00001 {
                normal_angle = atan2(-gradient.y, gradient.x);
                if normal_angle < 0.0 {
                    normal_angle += 2.0 * PI;
                }
            }
            let glare_angle = (normal_angle - PI / 4.0 + glass.glare_angle) * 2.0;
            let far_side = (glare_angle > PI * 1.5 && glare_angle < PI * 3.5) || glare_angle < PI * -0.5;
            let side_strength = select(1.2, 1.2 * glass.glare_opposite_strength, far_side);
            let angle_base = clamp((0.5 + sin(glare_angle) * 0.5) * side_strength * glass.glare_strength, 0.0, 1.0);
            let angle_strength = pow(angle_base, 0.1 + glass.glare_convergence * 2.0);
            let glare_light = angle_strength * glare_geometry;
            let glare_weight = glare_light * normal_length;
            var glare_lch = srgb_to_lch(mix(refracted, tint.rgb, tint.a * 0.5));
            // The source gradient magnitude belongs to the final blend only;
            // putting it in the LCH lightness increase would amplify it twice.
            glare_lch.x = clamp(glare_lch.x + 150.0 * glare_light, 0.0, 120.0);
            glare_lch.y += 30.0 * glare_light;
            color = mix(color, lch_to_srgb(glare_lch), glare_weight);
        }
    } else {
        color = mix(sample_blurred(uv), tint.rgb, tint.a * 0.8);
    }
    return vec4<f32>(srgb_to_linear_rgb(color), coverage);
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let local = (in.uv - vec2<f32>(0.5)) * in.size;
    let distance = shape_distance(local, in.size);
    let coverage = coverage_at(distance);
    var gradient = vec2<f32>(0.0);
    if coverage > 0.0 && needs_gradient(distance) {
        gradient = shape_gradient(local, in.size);
    }
    let pixel = shade_glass(in.position.xy, distance, gradient, coverage);
    return vec4<f32>(encode_color(pixel.rgb), pixel.a);
}

@fragment
fn fragment_group(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let position = in.position.xy;
    let distance = group_distance(position);
    let coverage = coverage_at(distance);
    var gradient = vec2<f32>(0.0);
    if coverage > 0.0 && needs_gradient(distance) {
        gradient = group_gradient(position);
    }
    let pixel = shade_glass(position, distance, gradient, coverage);

    // Shade the union silhouette outside the glass rather than the overlay's
    // rectangular bounds. This keeps the shadow attached during blob merging.
    var shadow_alpha = 0.0;
    if group.shadow_color.a > 0.0 && coverage < 1.0 {
        let shadow_distance = group_distance(position - group.shadow_offset) - group.shadow_spread;
        if group.shadow_blur > 0.0 {
            shadow_alpha = group.shadow_color.a * exp(-max(shadow_distance, 0.0) / group.shadow_blur);
        } else if shadow_distance <= 0.0 {
            shadow_alpha = group.shadow_color.a;
        }
    }
    let alpha = pixel.a + shadow_alpha * (1.0 - pixel.a);
    if alpha <= 0.0 {
        return vec4<f32>(0.0);
    }
    let color = (encode_color(pixel.rgb) * pixel.a
        + encode_color(group.shadow_color.rgb) * shadow_alpha * (1.0 - pixel.a)) / alpha;
    return vec4<f32>(color, alpha);
}
