struct Camera {
    view_projection: mat4x4<f32>,
    eye_position: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) model_0: vec4<f32>,
    @location(3) model_1: vec4<f32>,
    @location(4) model_2: vec4<f32>,
    @location(5) model_3: vec4<f32>,
    @location(6) material_tint: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world_position: vec3<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let model = mat4x4<f32>(
        input.model_0,
        input.model_1,
        input.model_2,
        input.model_3,
    );
    let world_position = model * vec4<f32>(input.position, 1.0);
    output.clip_position = camera.view_projection * world_position;
    output.world_position = world_position.xyz;
    output.color = input.color * input.material_tint.rgb;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let world_normal = normalize(cross(dpdx(input.world_position), dpdy(input.world_position)));
    let light_direction = normalize(vec3<f32>(-0.45, 0.82, 0.35));
    let diffuse = abs(dot(world_normal, light_direction));
    let ambient = 0.28;
    let lighting = ambient + (1.0 - ambient) * diffuse;
    let lit_color = input.color * lighting;
    let view_distance = distance(input.world_position, camera.eye_position.xyz);
    let fog_amount = smoothstep(20.0, 72.0, view_distance);
    let fog_color = vec3<f32>(0.018, 0.032, 0.075);
    let atmospheric_color = mix(lit_color, fog_color, fog_amount);
    return vec4<f32>(atmospheric_color, 1.0);
}
