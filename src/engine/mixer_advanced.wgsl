// Advanced video mixer shader with blend modes and keying
// Samples both source and destination textures for full control over mixing

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Uniforms for mixing parameters
struct MixParams {
    // Blend mode: 0=Normal, 1=Add, 2=Multiply, 3=Screen, 4=Overlay, 5=SoftLight, 6=HardLight
    // Keying modes: 10=ChromaKey, 11=LumaKey
    blend_mode: u32,
    
    // Opacity of source layer (0.0 - 1.0)
    opacity: f32,
    
    // Chroma key parameters
    key_color_r: f32,      // Key color (RGB)
    key_color_g: f32,
    key_color_b: f32,
    key_threshold: f32,    // Distance threshold for keying (0.0 - 1.0)
    key_smoothness: f32,   // Edge smoothness (0.0 - 1.0)
    
    // Luma key parameters
    luma_threshold: f32,   // Brightness threshold (0.0 - 1.0)
    luma_smoothness: f32,  // Edge smoothness (0.0 - 1.0)
    luma_invert: u32,      // Invert luma key (0 or 1)
    
    // Color space (0 = RGB, 1 = YCoCg)
    color_space: u32,
    
    // Padding to 48 bytes (3 vec4s)
    _padding: f32,
}

@group(0) @binding(0)
var source_texture: texture_2d<f32>;
@group(0) @binding(1)
var dest_texture: texture_2d<f32>;  // The accumulated output so far
@group(0) @binding(2)
var input_sampler: sampler;
@group(0) @binding(3)
var<uniform> params: MixParams;

// Vertex shader - fullscreen quad
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var pos = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0)
    );
    
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0)
    );
    
    var out: VertexOutput;
    out.position = vec4<f32>(pos[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

// Convert YCoCg to RGB
fn ycocg_to_rgb(color: vec4<f32>) -> vec3<f32> {
    let co = color.r;
    let cg = color.g;
    let scale_encoded = color.b;
    let y = color.a;
    
    let scale = (scale_encoded * (255.0 / 8.0)) + 1.0;
    let co_signed = (co - 0.5) / scale;
    let cg_signed = (cg - 0.5) / scale;
    let y_signed = y - 0.5;
    
    var rgb: vec3<f32>;
    rgb.r = y_signed + co_signed - cg_signed + 0.5;
    rgb.g = y_signed + cg_signed + 0.5;
    rgb.b = y_signed - co_signed - cg_signed + 0.5;
    
    return rgb;
}

// RGB to HSV conversion (for better chroma keying)
fn rgb_to_hsv(rgb: vec3<f32>) -> vec3<f32> {
    let c_max = max(max(rgb.r, rgb.g), rgb.b);
    let c_min = min(min(rgb.r, rgb.g), rgb.b);
    let delta = c_max - c_min;
    
    var h: f32 = 0.0;
    var s: f32 = 0.0;
    let v = c_max;
    
    if (delta > 0.0) {
        s = delta / c_max;
        
        if (rgb.r == c_max) {
            h = (rgb.g - rgb.b) / delta;
            if (h < 0.0) { h = h + 6.0; }
        } else if (rgb.g == c_max) {
            h = ((rgb.b - rgb.r) / delta) + 2.0;
        } else {
            h = ((rgb.r - rgb.g) / delta) + 4.0;
        }
        h = h / 6.0;
    }
    
    return vec3<f32>(h, s, v);
}

// Calculate color distance (perceptual)
fn color_distance(c1: vec3<f32>, c2: vec3<f32>) -> f32 {
    // Use weighted RGB distance (closer to perceptual)
    let diff = c1 - c2;
    let weights = vec3<f32>(0.299, 0.587, 0.114);
    return sqrt(dot(diff * diff, weights));
}

// Chroma key function
fn chroma_key(src: vec4<f32>, dst: vec4<f32>) -> vec4<f32> {
    let key_rgb = vec3<f32>(params.key_color_r, params.key_color_g, params.key_color_b);
    
    // Calculate distance from key color
    let dist = color_distance(src.rgb, key_rgb);
    
    // Calculate alpha based on threshold and smoothness
    let threshold = params.key_threshold;
    let smoothness = max(params.key_smoothness, 0.001);
    
    // Smooth step for edge softness
    var alpha = smoothstep(threshold, threshold + smoothness, dist);
    
    // Apply opacity
    alpha = alpha * params.opacity;
    
    // Composite over destination
    return vec4<f32>(src.rgb, src.a * alpha) + dst * (1.0 - src.a * alpha);
}

// Luma key function
fn luma_key(src: vec4<f32>, dst: vec4<f32>) -> vec4<f32> {
    // Calculate luminance
    let luminance = dot(src.rgb, vec3<f32>(0.299, 0.587, 0.114));
    
    let threshold = params.luma_threshold;
    let smoothness = max(params.luma_smoothness, 0.001);
    
    // Calculate alpha based on luminance
    var alpha: f32;
    if (params.luma_invert != 0u) {
        // Invert: keep dark areas
        alpha = 1.0 - smoothstep(threshold, threshold + smoothness, luminance);
    } else {
        // Normal: keep bright areas
        alpha = smoothstep(threshold, threshold + smoothness, luminance);
    }
    
    // Apply opacity
    alpha = alpha * params.opacity;
    
    // Composite over destination
    return vec4<f32>(src.rgb, src.a * alpha) + dst * (1.0 - src.a * alpha);
}

// Blend modes (operate on RGB only, preserve destination alpha for now)
fn blend_normal(src: vec3<f32>, dst: vec3<f32>, opacity: f32) -> vec3<f32> {
    return mix(dst, src, opacity);
}

fn blend_add(src: vec3<f32>, dst: vec3<f32>, opacity: f32) -> vec3<f32> {
    return dst + src * opacity;
}

fn blend_multiply(src: vec3<f32>, dst: vec3<f32>, opacity: f32) -> vec3<f32> {
    return mix(dst, dst * src, opacity);
}

fn blend_screen(src: vec3<f32>, dst: vec3<f32>, opacity: f32) -> vec3<f32> {
    let screen = 1.0 - (1.0 - dst) * (1.0 - src);
    return mix(dst, screen, opacity);
}

fn blend_overlay(src: vec3<f32>, dst: vec3<f32>, opacity: f32) -> vec3<f32> {
    var overlay: vec3<f32>;
    for (var i = 0; i < 3; i = i + 1) {
        if (dst[i] < 0.5) {
            overlay[i] = 2.0 * dst[i] * src[i];
        } else {
            overlay[i] = 1.0 - 2.0 * (1.0 - dst[i]) * (1.0 - src[i]);
        }
    }
    return mix(dst, overlay, opacity);
}

fn blend_soft_light(src: vec3<f32>, dst: vec3<f32>, opacity: f32) -> vec3<f32> {
    var result: vec3<f32>;
    for (var i = 0; i < 3; i = i + 1) {
        if (src[i] < 0.5) {
            result[i] = dst[i] - (1.0 - 2.0 * src[i]) * dst[i] * (1.0 - dst[i]);
        } else {
            let d = select(sqrt(dst[i]), dst[i], dst[i] < 0.25);
            result[i] = dst[i] + (2.0 * src[i] - 1.0) * (d - dst[i]);
        }
    }
    return mix(dst, result, opacity);
}

fn blend_hard_light(src: vec3<f32>, dst: vec3<f32>, opacity: f32) -> vec3<f32> {
    var result: vec3<f32>;
    for (var i = 0; i < 3; i = i + 1) {
        if (src[i] < 0.5) {
            result[i] = 2.0 * dst[i] * src[i];
        } else {
            result[i] = 1.0 - 2.0 * (1.0 - dst[i]) * (1.0 - src[i]);
        }
    }
    return mix(dst, result, opacity);
}

fn blend_difference(src: vec3<f32>, dst: vec3<f32>, opacity: f32) -> vec3<f32> {
    return mix(dst, abs(dst - src), opacity);
}

fn blend_lighten(src: vec3<f32>, dst: vec3<f32>, opacity: f32) -> vec3<f32> {
    return mix(dst, max(dst, src), opacity);
}

fn blend_darken(src: vec3<f32>, dst: vec3<f32>, opacity: f32) -> vec3<f32> {
    return mix(dst, min(dst, src), opacity);
}

// Fragment shader
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample source texture
    var src_tex = textureSample(source_texture, input_sampler, in.uv);
    
    // Sample destination texture (accumulated output so far)
    var dst_tex = textureSample(dest_texture, input_sampler, in.uv);
    
    // Handle color space conversion
    var src_rgb: vec3<f32>;
    if (params.color_space == 1u) {
        src_rgb = ycocg_to_rgb(src_tex);
    } else {
        src_rgb = src_tex.rgb;
    }
    
    // Apply keying or blending based on mode
    var result: vec4<f32>;
    let mode = params.blend_mode;
    
    if (mode == 10u) {
        // Chroma key
        result = chroma_key(vec4<f32>(src_rgb, src_tex.a), dst_tex);
    } else if (mode == 11u) {
        // Luma key
        result = luma_key(vec4<f32>(src_rgb, src_tex.a), dst_tex);
    } else {
        // Standard blend modes
        var blended: vec3<f32>;
        let opacity = params.opacity * src_tex.a;
        
        switch (mode) {
            case 0u: { blended = blend_normal(src_rgb, dst_tex.rgb, opacity); }
            case 1u: { blended = blend_add(src_rgb, dst_tex.rgb, opacity); }
            case 2u: { blended = blend_multiply(src_rgb, dst_tex.rgb, opacity); }
            case 3u: { blended = blend_screen(src_rgb, dst_tex.rgb, opacity); }
            case 4u: { blended = blend_overlay(src_rgb, dst_tex.rgb, opacity); }
            case 5u: { blended = blend_soft_light(src_rgb, dst_tex.rgb, opacity); }
            case 6u: { blended = blend_hard_light(src_rgb, dst_tex.rgb, opacity); }
            case 7u: { blended = blend_difference(src_rgb, dst_tex.rgb, opacity); }
            case 8u: { blended = blend_lighten(src_rgb, dst_tex.rgb, opacity); }
            case 9u: { blended = blend_darken(src_rgb, dst_tex.rgb, opacity); }
            default: { blended = blend_normal(src_rgb, dst_tex.rgb, opacity); }
        }
        
        result = vec4<f32>(blended, max(dst_tex.a, src_tex.a * opacity));
    }
    
    return result;
}
