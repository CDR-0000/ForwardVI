//! `victory bvh-check`: the console's ray-tracing structure builder
//! (`XboxBvhBuilder.dll`, from the game folder) builds a structure over a
//! test mesh on this machine's GPU, through the layer as in a run (its
//! programs translated, title memory by address), without the title. The
//! result is then read back and checked on the CPU with the reference of
//! the node test (`victory_shader::bvh`):
//!
//! - the walk from the root reaches every input triangle, once;
//! - every child's box holds what lies under it;
//! - rays through the mesh find the same nearest hit as a test of every
//!   triangle.
//!
//! The structure's layout is found, not assumed: the root is the box node
//! the walk from which reaches the most triangles, and node pointers are
//! taken relative to the structure's address (`bvh::node_address`).

use std::collections::HashSet;

use anyhow::{Context, Result, bail, ensure};
use victory_shader::bvh;

/// `D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS`.
#[repr(C)]
#[derive(Clone, Copy)]
struct Inputs {
    kind: u32,
    flags: u32,
    descs: u32,
    layout: u32,
    geometry: *const Geometry,
}

/// `D3D12_RAYTRACING_GEOMETRY_DESC` of triangles.
#[repr(C)]
struct Geometry {
    kind: u32,
    flags: u32,
    transform: u64,
    index_format: u32,
    vertex_format: u32,
    index_count: u32,
    vertex_count: u32,
    index_buffer: u64,
    vertex_buffer: u64,
    vertex_stride: u64,
}
const _: () = assert!(size_of::<Geometry>() == 56);

/// `D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC`.
#[repr(C)]
struct Build {
    dest: u64,
    inputs: Inputs,
    source: u64,
    scratch: u64,
}
const _: () = assert!(size_of::<Build>() == 48);

/// `D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO`.
#[repr(C)]
#[derive(Default, Debug)]
struct Prebuild {
    result: u64,
    scratch: u64,
    update_scratch: u64,
}

const BOTTOM_LEVEL: u32 = 1;
const PREFER_FAST_TRACE: u32 = 4;
const TRIANGLES: u32 = 0;
const OPAQUE: u32 = 1;
const R32G32B32_FLOAT: u32 = 6;
const R32_UINT: u32 = 42;

const CTOR: &str = "??0XboxBvhBuilder@@QEAA@PEAUID3D12Device11@@_N@Z";
const PREBUILD: &str = "?GetRaytracingAccelerationStructurePrebuildInfo@XboxBvhBuilder@@QEAAXPEBUD3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS@@PEBUD3D12XBOX_XDXR_STREAM_DESC@@PEAUD3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO@@@Z";
const BUILD: &str = "?BuildRaytracingAccelerationStructure@XboxBvhBuilder@@QEAAXPEAUID3D12GraphicsCommandList4@@PEBUD3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC@@IPEBUD3D12_RAYTRACING_ACCELERATION_STRUCTURE_POSTBUILD_INFO_DESC@@@Z";

type Ctor = unsafe extern "system" fn(*mut u8, *const u8, bool) -> *mut u8;
type PrebuildFn = unsafe extern "system" fn(*mut u8, *const Inputs, *const u8, *mut Prebuild);
type BuildFn = unsafe extern "system" fn(*mut u8, *const u8, *const Build, u32, *const u8);

pub fn run(size: u32, boxes: bool, builds: u32) -> Result<()> {
    use victory_hle::{HleResolver, HostPolicy};
    use victory_loader::{UnimplementedPolicy, WithTitleDlls};

    // The builder runs only when asked, and needs the exact first-lane
    // read of its scans on 32-lane GPUs (`emit/lower.rs`).
    for (name, value) in [("VICTORY_BVH_BUILDER", "1"), ("VICTORY_FIRSTLANE_OWN", "1")] {
        if std::env::var_os(name).is_none() {
            // SAFETY: at start-up, before any other thread reads the environment.
            unsafe { std::env::set_var(name, value) };
        }
    }
    let cfg = victory_core::Config::load()?;
    let game = cfg.game_dir.clone().context("game_dir is not configured (victory.toml)")?;
    let images = cfg.cache_subdir("images")?;
    victory_hle::modules::d3d12x::use_shader_cache(cfg.cache_subdir("shaders")?);

    // The DLL, bound to our runtime as in a run.
    let resolver = HleResolver::new(HostPolicy::default());
    let dlls = victory_loader::load_title_dll("XboxBvhBuilder.dll", &game, &images, &|_| true)?;
    ensure!(!dlls.is_empty(), "no XboxBvhBuilder.dll in {}", game.display());
    let with_dlls = WithTitleDlls { inner: &resolver, dlls: &dlls };
    for (name, dll) in &dlls {
        let report = dll.bind_imports(&with_dlls, UnimplementedPolicy::ReturnZero)?;
        println!("bound          {name}: {} hle, {} host, {} missing", report.hle, report.host, report.missing.len());
        for m in &report.missing {
            println!("  missing      {}!{}", m.dll, m.symbol);
        }
        // SAFETY: its imports are bound.
        unsafe { dll.attach() }?;
    }
    let builder_dll = &dlls.last().context("no builder")?.1;
    let export = |name: &str| builder_dll.export(name).with_context(|| format!("no export {name}"));

    // The device as the title makes it (through the runtime's creation).
    let create = match victory_loader::ImportResolver::resolve(
        &resolver,
        "d3d12_xs.dll",
        &victory_pe::ImportSymbol::Name { hint: 0, name: "D3D12XboxCreateDevice".into() },
    ) {
        victory_loader::Resolution::Found { address, .. } => address,
        _ => bail!("no D3D12XboxCreateDevice"),
    };
    // SAFETY: the runtime's device creation, with its own signature.
    let gpu = victory_gpu::testbed::TitleGpu::new(unsafe { std::mem::transmute(create) })
        .map_err(anyhow::Error::msg)?;

    // The mesh: `size` x `size` quads over a bumpy surface, or (`boxes`) a
    // ground of `size` x `size` quads with two boxes standing on it.
    let (vertices, indices) = if boxes { scene(size) } else { mesh(size) };
    let triangles = indices.len() / 3;
    println!("mesh           {} vertices, {triangles} triangles", vertices.len());
    // Write-watched title memory outside buffer objects, where this tree
    // gives the builder's stores places on the card (`crate::memory`'s
    // arenas). `VICTORY_BVH_CHECK_BUFFERS=1`: buffer objects made through the
    // device instead (their pages take the builder's stores only once one of
    // its dispatches was given the buffer as a written root view).
    let raw = std::env::var_os("VICTORY_BVH_CHECK_BUFFERS").is_none_or(|v| v != "1");
    let title = |bytes: usize| {
        if raw {
            victory_gpu::testbed::title_memory(bytes)
        } else {
            victory_gpu::testbed::title_buffer(&gpu, bytes as u64)
        }
        .map_err(anyhow::Error::msg)
    };
    let vb = title(vertices.len() * 12)?;
    let ib = title(indices.len() * 4)?;
    // SAFETY: fresh mappings of at least these sizes.
    unsafe {
        std::ptr::copy_nonoverlapping(vertices.as_ptr().cast::<u8>(), vb, vertices.len() * 12);
        std::ptr::copy_nonoverlapping(indices.as_ptr().cast::<u8>(), ib, indices.len() * 4);
    }
    let geometry = Geometry {
        kind: TRIANGLES,
        flags: OPAQUE,
        transform: 0,
        index_format: R32_UINT,
        vertex_format: R32G32B32_FLOAT,
        index_count: indices.len() as u32,
        vertex_count: vertices.len() as u32,
        index_buffer: ib as u64,
        vertex_buffer: vb as u64,
        vertex_stride: 12,
    };
    let inputs = Inputs { kind: BOTTOM_LEVEL, flags: PREFER_FAST_TRACE, descs: 1, layout: 0, geometry: &geometry };

    // The builder object: its size is not known; a generous zeroed block.
    let object = Box::leak(vec![0u64; 1 << 17].into_boxed_slice()).as_mut_ptr().cast::<u8>();
    // SAFETY: the exports have these signatures (their decorated names say
    // so); the object block is larger than the class.
    let prebuild = unsafe {
        let ctor: Ctor = std::mem::transmute(export(CTOR)?);
        ctor(object, gpu.device.cast(), false);
        let get: PrebuildFn = std::mem::transmute(export(PREBUILD)?);
        let mut info = Prebuild::default();
        get(object, &inputs, std::ptr::null(), &mut info);
        info
    };
    println!("prebuild       {prebuild:?}");
    ensure!(prebuild.result > 0 && prebuild.scratch > 0, "the builder gave no sizes");
    let dest = title(prebuild.result as usize)?;
    let scratch = title(prebuild.scratch as usize)?;
    let build = Build { dest: dest as u64, inputs, source: 0, scratch: scratch as u64 };
    println!(
        "memory         vertices {:#x}, indices {:#x}, result {dest:#x?}, scratch {scratch:#x?}, builder object {object:#x?}",
        vb as usize, ib as usize
    );
    // `builds` times, a frame apart, as a title that builds every frame
    // (the layer makes the pages shaders found missing ready by the next).
    for n in 0..builds.max(1) {
        // SAFETY: as above; the list is in the recording state.
        unsafe {
            let record: BuildFn = std::mem::transmute(export(BUILD)?);
            record(object, gpu.list.cast(), &build, 0, std::ptr::null());
        }
        let started = std::time::Instant::now();
        gpu.run().map_err(anyhow::Error::msg)?;
        let (dispatches, draws) = victory_gpu::testbed::counts();
        println!(
            "built          #{} in {:.1} ms (recording, translation and the GPU): {dispatches} dispatches, {draws} draws",
            n + 1,
            started.elapsed().as_secs_f64() * 1e3
        );
    }

    // An empty submission and a pause: whatever comes back late is in.
    if let Ok(ms) = std::env::var("VICTORY_BVH_CHECK_SETTLE_MS")
        && let Ok(ms) = ms.parse::<u64>()
    {
        gpu.run().map_err(anyhow::Error::msg)?;
        std::thread::sleep(std::time::Duration::from_millis(ms));
        gpu.run().map_err(anyhow::Error::msg)?;
    }
    // What the GPU's copy holds: where the programs that walk structures
    // read them (this tree does not copy the builder's results back into
    // title memory). `VICTORY_BVH_CHECK_TITLE=1`: title memory instead.
    let from_title = std::env::var_os("VICTORY_BVH_CHECK_TITLE").is_some_and(|v| v == "1");
    let result = if from_title {
        // SAFETY: the structure's memory, now settled.
        unsafe { std::slice::from_raw_parts(dest, prebuild.result as usize) }.to_vec()
    } else {
        victory_gpu::testbed::read_card(dest as u64, prebuild.result).map_err(anyhow::Error::msg)?
    };
    let used = result.iter().rposition(|&b| b != 0).map_or(0, |n| n + 1);
    println!("result         {used} of {} bytes written", result.len());
    let dump = cfg.cache_dir.join("bvh-check.bin");
    std::fs::write(&dump, &result).with_context(|| format!("writing {}", dump.display()))?;
    println!("saved          {}", dump.display());
    // SAFETY: the scratch memory, settled as the result is.
    let scratch_bytes = unsafe { std::slice::from_raw_parts(scratch, prebuild.scratch as usize) };
    std::fs::write(cfg.cache_dir.join("bvh-check-scratch.bin"), scratch_bytes)?;
    ensure!(used > 0, "the builder wrote nothing: its stages did not run (see the log)");
    check(&result, dest as u64, &vertices, &indices, &cfg.cache_dir)
}

/// A grid of `n` x `n` quads, two triangles each, over z = a few sines.
fn mesh(n: u32) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut vertices = Vec::new();
    for y in 0..=n {
        for x in 0..=n {
            let (fx, fy) = (x as f32, y as f32);
            vertices.push([fx, fy, (fx * 0.7).sin() + (fy * 0.4).cos() * 0.5]);
        }
    }
    let at = |x: u32, y: u32| y * (n + 1) + x;
    let mut indices = Vec::new();
    for y in 0..n {
        for x in 0..n {
            indices.extend([at(x, y), at(x + 1, y), at(x, y + 1)]);
            indices.extend([at(x + 1, y), at(x + 1, y + 1), at(x, y + 1)]);
        }
    }
    (vertices, indices)
}

/// A ground of `n` x `n` quads over [0, 16]², and two boxes on it (a cube
/// and a tall one), twelve triangles each.
fn scene(n: u32) -> (Vec<[f32; 3]>, Vec<u32>) {
    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
    let step = 16.0 / n as f32;
    for y in 0..=n {
        for x in 0..=n {
            vertices.push([x as f32 * step, y as f32 * step, 0.0]);
        }
    }
    let at = |x: u32, y: u32| y * (n + 1) + x;
    for y in 0..n {
        for x in 0..n {
            indices.extend([at(x, y), at(x + 1, y), at(x, y + 1)]);
            indices.extend([at(x + 1, y), at(x + 1, y + 1), at(x, y + 1)]);
        }
    }
    let mut cuboid = |low: [f32; 3], high: [f32; 3]| {
        let first = vertices.len() as u32;
        for c in 0..8u32 {
            vertices.push(std::array::from_fn(|a| if c >> a & 1 == 0 { low[a] } else { high[a] }));
        }
        // Corners by bits x, y, z; each face as two triangles.
        for face in [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]] {
            let [a, b, c, d] = face.map(|k| first + k);
            indices.extend([a, b, c, a, c, d]);
        }
    };
    cuboid([4.0, 5.0, 0.0], [7.0, 8.0, 3.0]);
    cuboid([9.5, 8.5, 0.0], [11.0, 10.0, 5.5]);
    (vertices, indices)
}

/// The triangles of a triangle node a pointer of `kind` (0-3) reaches: that
/// one, and the next if the node holds a pair. As this builder writes them,
/// dword 12 is 1 for a node of one triangle and 0x81 for two (an input quad:
/// the triangle numbers in dwords 13 and 14, both vertex maps in dword 15),
/// and the parent's pointer names the first.
fn leaf_triangles(words: &[u32; 28], kind: u32) -> impl Iterator<Item = u32> {
    let pair = words[12] & 0x80 != 0 && kind < 3;
    kind..=kind + u32::from(pair)
}

/// The 28 dwords of the node at byte `at` of `result` (zeros past its end).
fn node(result: &[u8], at: usize) -> [u32; 28] {
    std::array::from_fn(|i| {
        let o = at + 4 * i;
        result.get(o..o + 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
    })
}

/// A triangle as a key: its three vertices' bits, sorted.
fn key(t: &[[f32; 3]; 3]) -> [[u32; 3]; 3] {
    let mut k = t.map(|v| v.map(f32::to_bits));
    k.sort_unstable();
    k
}

/// What a walk from `root` finds: the triangles of the leaves (as keys,
/// with repeats), the box nodes visited, and the boxes that do not hold
/// what lies under them.
struct Walk {
    triangles: Vec<[[u32; 3]; 3]>,
    boxes: usize,
    loose: usize,
    outside: usize,
}

fn walk(result: &[u8], base: u64, root: u64) -> Walk {
    let mut w = Walk { triangles: Vec::new(), boxes: 0, loose: 0, outside: 0 };
    let mut seen = HashSet::new();
    // (pointer, the box its parent gave it)
    let mut stack = vec![(root, None::<[f32; 6]>)];
    let word0 = (base >> 8) as u32;
    let word1 = (base >> 40) as u32;
    while let Some((pointer, bound)) = stack.pop() {
        if stack.len() > 1 << 16 || w.boxes > 1 << 20 {
            break;
        }
        let at = bvh::node_address(word0, word1, pointer).wrapping_sub(base) as usize;
        if at >= result.len() {
            w.outside += 1;
            continue;
        }
        let words = node(result, at);
        let kind = bvh::kind(pointer);
        let holds = |b: &[f32; 6], p: &[f32; 3]| (0..3).all(|a| b[a] - 1e-3 <= p[a] && p[a] <= b[3 + a] + 1e-3);
        match kind {
            4 | 5 => {
                if !seen.insert(pointer) {
                    continue;
                }
                w.boxes += 1;
                let boxes = if kind == 4 { bvh::boxes16(&words) } else { bvh::boxes32(&words) };
                for (c, b) in bvh::children(&words).iter().zip(boxes) {
                    if *c != bvh::NONE {
                        if let Some(parent) = bound
                            && !(holds(&parent, &[b[0], b[1], b[2]]) && holds(&parent, &[b[3], b[4], b[5]]))
                        {
                            w.loose += 1;
                        }
                        stack.push((u64::from(*c), Some(b)));
                    }
                }
            }
            0..=3 => {
                for k in leaf_triangles(&words, kind) {
                    let t = bvh::triangle_vertices(&words, k);
                    if let Some(b) = bound
                        && !t.iter().all(|p| holds(&b, p))
                    {
                        w.loose += 1;
                    }
                    w.triangles.push(key(&t));
                }
            }
            _ => {}
        }
    }
    w
}

/// A hit: its distance (infinite: none) and its triangle's unit normal.
type Hit = (f32, [f32; 3]);
const NO_HIT: Hit = (f32::INFINITY, [0.0; 3]);

fn normal(t: &[[f32; 3]; 3]) -> [f32; 3] {
    let (a, b) = (sub(t[1], t[0]), sub(t[2], t[0]));
    unit(cross(a, b))
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn unit(v: [f32; 3]) -> [f32; 3] {
    let l = dot(v, v).sqrt();
    v.map(|x| x / l)
}

/// The nearest hit of `ray` testing every triangle.
fn every(input: &[[[f32; 3]; 3]], ray: &bvh::Ray) -> Hit {
    let mut best = NO_HIT;
    for t in input {
        let [num, den, ..] = bvh::triangle(t, ray);
        let d = num / den;
        if d.is_finite() && d >= 0.0 && d < best.0.min(ray.extent) {
            best = (d, normal(t));
        }
    }
    best
}

/// The nearest hit of `ray` through the walk from `root`, as the trace
/// programs would find it with the node test.
fn trace(result: &[u8], base: u64, root: u64, ray: &bvh::Ray) -> Hit {
    let word0 = (base >> 8) as u32;
    let word1 = (base >> 40) as u32;
    let mut best = ray.extent;
    let mut hit = NO_HIT;
    let mut stack = vec![root];
    let mut steps = 0;
    while let Some(pointer) = stack.pop() {
        steps += 1;
        if steps > 1 << 20 {
            break;
        }
        let at = bvh::node_address(word0, word1, pointer).wrapping_sub(base) as usize;
        if at >= result.len() {
            continue;
        }
        let words = node(result, at);
        let kind = bvh::kind(pointer);
        let r = bvh::Ray { extent: best, ..*ray };
        match kind {
            4 | 5 => {
                let out = bvh::intersect(&words, kind, &r, true);
                stack.extend(out.iter().rev().filter(|&&c| c != bvh::NONE).map(|&c| u64::from(c)));
            }
            // The triangle the pointer names, and the one after it in a
            // node of two ([`leaf_triangles`]).
            0..=3 => {
                for k in leaf_triangles(&words, kind) {
                    let tri = bvh::triangle_vertices(&words, k);
                    let [num, den, ..] = bvh::triangle(&tri, &r);
                    let t = num / den;
                    if t.is_finite() && t >= 0.0 && t < best {
                        best = t;
                        hit = (t, normal(&tri));
                    }
                }
            }
            _ => {}
        }
    }
    hit
}

/// Pictures of the mesh lit by a sun, with ray-traced shadows: through the
/// structure (left), by testing every triangle (middle), and where the
/// first rays differ (right). Each pixel is a ray from the eye, and from
/// what it hits a second ray toward the sun: in shadow if that one hits
/// anything. A PNG at `path`.
fn render(
    path: &std::path::Path,
    bounds: ([f32; 3], [f32; 3]),
    walked: impl Fn(&bvh::Ray) -> Hit,
    every: impl Fn(&bvh::Ray) -> Hit,
) -> Result<()> {
    const W: usize = 400;
    const H: usize = 300;
    let (low, high) = bounds;
    let size = dot(sub(high, low), sub(high, low)).sqrt();
    let target = [(low[0] + high[0]) / 2.0, (low[1] + high[1]) / 2.0, low[2]];
    let eye = [target[0] - 0.3 * size, target[1] - 0.6 * size, target[2] + 0.45 * size];
    let sun = unit([-0.45, -0.3, 0.85]);
    let forward = unit(sub(target, eye));
    let right = unit(cross(forward, [0.0, 0.0, 1.0]));
    let up = cross(right, forward);
    let mut pixels = vec![[0u8; 3]; 3 * W * H];
    // The colour of a pixel: sky, or the surface lit by the sun unless a ray
    // toward it hits something.
    let colour = |ray: &bvh::Ray, hit: Hit, trace: &dyn Fn(&bvh::Ray) -> Hit| -> [u8; 3] {
        let (t, n) = hit;
        if t.is_infinite() {
            let k = (ray.direction[2] * 0.5 + 0.5).clamp(0.0, 1.0);
            return [(60.0 + 80.0 * k) as u8, (80.0 + 90.0 * k) as u8, (120.0 + 110.0 * k) as u8];
        }
        let n = if dot(n, ray.direction) > 0.0 { n.map(|x| -x) } else { n };
        let p: [f32; 3] = std::array::from_fn(|a| ray.origin[a] + t * ray.direction[a] + n[a] * 1e-3 * size);
        let lit = dot(n, sun).max(0.0);
        let shadowed = lit > 0.0 && trace(&bvh::Ray::new(p, sun, f32::INFINITY)).0.is_finite();
        let light = 0.18 + if shadowed { 0.0 } else { 0.82 * lit };
        // The ground warm, the boxes' tops and sides by their facing.
        let base = if n[2] > 0.9 && p[2] < low[2] + 2e-3 * size { [0.85, 0.78, 0.65] } else { [0.55, 0.7, 0.9] };
        base.map(|c| (255.0 * (c * light).min(1.0)) as u8)
    };
    for y in 0..H {
        for x in 0..W {
            let (u, v) = ((x as f32 + 0.5) / W as f32 * 2.0 - 1.0, 1.0 - (y as f32 + 0.5) / H as f32 * 2.0);
            let d = unit(std::array::from_fn(|a| forward[a] + 0.42 * (u * 1.33 * right[a] + v * up[a])));
            let ray = bvh::Ray::new(eye, d, f32::INFINITY);
            let (ha, hb) = (walked(&ray), every(&ray));
            let (a, b) = (ha.0, hb.0);
            let same = (a.is_infinite() && b.is_infinite()) || (a - b).abs() <= 1e-3 * b.abs().max(1.0);
            pixels[y * 3 * W + x] = colour(&ray, ha, &walked);
            pixels[y * 3 * W + W + x] = colour(&ray, hb, &every);
            pixels[y * 3 * W + 2 * W + x] = match (same, a.is_infinite(), b.is_infinite()) {
                (true, ..) => pixels[y * 3 * W + W + x].map(|c| c / 3),
                (false, true, false) => [230, 40, 40], // the walk misses a hit
                (false, false, true) => [240, 200, 40], // the walk hits nothing there
                (false, ..) => [240, 120, 30],          // another distance
            };
        }
    }
    std::fs::write(path, png(3 * W, H, &pixels)).with_context(|| format!("writing {}", path.display()))
}

/// An RGB PNG, stored without compression.
fn png(width: usize, height: usize, pixels: &[[u8; 3]]) -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut c = 0xffff_ffffu32;
        for &b in bytes {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    let chunk = |out: &mut Vec<u8>, kind: &[u8], data: &[u8]| {
        out.extend((data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend(kind);
        out.extend(data);
        let c = crc(&out[start..]);
        out.extend(c.to_be_bytes());
    };
    let mut raw = Vec::new();
    for row in pixels.chunks(width) {
        raw.push(0);
        raw.extend(row.iter().flatten());
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<_> = raw.chunks(65535).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        z.extend((block.len() as u16).to_le_bytes());
        z.extend((!(block.len() as u16)).to_le_bytes());
        z.extend(*block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    z.extend((b << 16 | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend((width as u32).to_be_bytes());
    header.extend((height as u32).to_be_bytes());
    header.extend([8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn check(result: &[u8], base: u64, vertices: &[[f32; 3]], indices: &[u32], out: &std::path::Path) -> Result<()> {
    let input: Vec<[[f32; 3]; 3]> = indices
        .chunks_exact(3)
        .map(|t| [vertices[t[0] as usize], vertices[t[1] as usize], vertices[t[2] as usize]])
        .collect();
    let wanted: HashSet<_> = input.iter().map(key).collect();

    // The root: of every 64-byte place read as a box node of either kind,
    // the one whose walk reaches the most input triangles.
    let mut best: Option<(usize, u64)> = None;
    for at in (0..result.len()).step_by(64) {
        for kind in [5u64, 4] {
            let pointer = ((at as u64) >> 3) | kind;
            let words = node(result, at);
            let children = bvh::children(&words);
            if children.iter().all(|&c| c == bvh::NONE || c == 0) {
                continue;
            }
            let w = walk(result, base, pointer);
            let found = w.triangles.iter().filter(|t| wanted.contains(*t)).count();
            if best.is_none_or(|(n, _)| found > n) {
                best = Some((found, pointer));
            }
        }
    }
    // The header's dword 12 names the root (seen in the results: a box
    // node whose boxes are the mesh's quarters); the search is the check.
    let named = u64::from(node(result, 0)[12]);
    let best = match best {
        Some((n, _)) if matches!(bvh::kind(named), 4 | 5) => {
            let w = walk(result, base, named);
            let found = w.triangles.iter().filter(|t| wanted.contains(*t)).count();
            if found >= n { Some((found, named)) } else { best }
        }
        other => other,
    };
    let Some((_, root)) = best else {
        bail!("no box node in the result");
    };
    let w = walk(result, base, root);
    let found: HashSet<_> = w.triangles.iter().copied().collect();
    let reached = wanted.intersection(&found).count();
    let foreign = found.difference(&wanted).count();
    let repeats = w.triangles.len() - found.len();
    println!(
        "root           byte {:#x} ({} box node)",
        (root >> 3) << 6,
        if bvh::kind(root) == 5 { "32-bit" } else { "16-bit" }
    );
    println!(
        "walk           {} box nodes; {reached} of {} triangles reached, {foreign} not in the mesh, {repeats} repeated; {} boxes not holding what is under them; {} pointers past the result",
        w.boxes,
        wanted.len(),
        w.loose,
        w.outside
    );

    // Rays from above the surface, down and slanted, on a lattice.
    let (mut agree, mut rays, mut first_bad) = (0, 0, None);
    let span = vertices.iter().map(|v| v[0]).fold(0f32, f32::max);
    for i in 0..400 {
        let (u, v) = ((i % 20) as f32 + 0.37, (i / 20) as f32 + 0.61);
        let origin = [u * span / 20.0, v * span / 20.0, 5.0];
        let direction = [0.13 * ((i % 7) as f32 - 3.0), 0.11 * ((i % 5) as f32 - 2.0), -1.0];
        let ray = bvh::Ray::new(origin, direction, f32::INFINITY);
        let brute = every(&input, &ray).0;
        let walked = trace(result, base, root, &ray).0;
        rays += 1;
        if (brute.is_infinite() && walked.is_infinite()) || (brute - walked).abs() <= 1e-4 * brute.abs().max(1.0) {
            agree += 1;
        } else if first_bad.is_none() {
            first_bad = Some((origin, direction, brute, walked));
        }
    }
    println!("rays           {agree} of {rays} find the same nearest hit as a test of every triangle");
    let picture = out.join("bvh-check.png");
    let bounds = vertices.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), v| {
        (std::array::from_fn(|a| lo[a].min(v[a])), std::array::from_fn(|a| hi[a].max(v[a])))
    });
    render(&picture, bounds, |ray| trace(result, base, root, ray), |ray| every(&input, ray))?;
    println!(
        "picture        {} (sun and ray-traced shadows. left: through the built structure; middle: every triangle tested; right: where the first rays differ, red a missed hit, orange another distance, yellow a hit from nothing)",
        picture.display()
    );
    if let Some((o, d, brute, walked)) = first_bad {
        println!("  first miss   origin {o:?} direction {d:?}: every triangle {brute}, the walk {walked}");
    }
    let ok = reached == wanted.len() && foreign == 0 && w.loose == 0 && agree == rays;
    println!("{}", if ok { "PASS" } else { "FAIL" });
    ensure!(ok, "the structure is not right");
    Ok(())
}
