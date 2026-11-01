//! Fractionally placed native block vertices retain canonical terrain material addresses.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub origin: [i32; 3],
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub normal: [f32; 3],
    pub material: u32,
    pub light: u32,
    pub flags: u32,
    pub biome: u32,
}
const _: () = assert!(std::mem::size_of::<Vertex>() == 60);
