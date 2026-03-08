// Simple video mixer shader
// Renders a fullscreen quad with texture sampling
// Supports RGB and YCoCg color spaces

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0)
var input_texture: texture_2d<f32>;
@group(0) @binding(1)
var input_sampler: sampler;

// Color space flag (0 = RGB, 1 = YCoCg) - passed as uniform
@group(0) @binding(2)
var<uniform> color_space: u32;

// Vertex shader - fullscreen quad
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Large triangle covering entire viewport
    // Triangle 1: (-1,-1), (3,-1), (-1,3) covers [-1,1] x [-1,1]
    var pos = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
        // Second triangle (not used with large triangle trick)
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0)
    );
    
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0), // Bottom-left (flipped Y)
        vec2<f32>(2.0, 1.0), // Bottom-right
        vec2<f32>(0.0, -1.0), // Top-left
        // Second triangle
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
// YCoCg DXT5 stores:
//   - Co in red channel (5-bit precision)
//   - Cg in green channel (6-bit precision)  
//   - Scale in blue channel
//   - Y (luminance) in alpha channel (8-bit precision)
fn ycocg_to_rgb(color: vec4<f32>) -> vec3<f32> {
    // Extract components
    let co = color.r;
    let cg = color.g;
    let scale_encoded = color.b;
    let y = color.a;
    
    // Decode scale factor: scale = (blue * 255/8) + 1
    // Scale is stored as (scale-1)*8, so scale = 1, 2, or 4
    let scale = (scale_encoded * (255.0 / 8.0)) + 1.0;
    
    // Convert from [0,1] to signed [-0.5, 0.5] range, then apply scale
    let co_signed = (co - 0.5) / scale;
    let cg_signed = (cg - 0.5) / scale;
    let y_signed = y - 0.5;
    
    // YCoCg to RGB matrix conversion
    // R = Y + Co - Cg
    // G = Y + Cg
    // B = Y - Co - Cg
    var rgb: vec3<f32>;
    rgb.r = y_signed + co_signed - cg_signed + 0.5;
    rgb.g = y_signed + cg_signed + 0.5;
    rgb.b = y_signed - co_signed - cg_signed + 0.5;
    
    return rgb;
}

// Fragment shader - texture sample with color space conversion
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(input_texture, input_sampler, in.uv);
    
    if (color_space == 1u) {
        // YCoCg color space - convert to RGB
        let rgb = ycocg_to_rgb(tex_color);
        return vec4<f32>(rgb, 1.0);
    } else {
        // RGB color space - passthrough
        return tex_color;
    }
}
