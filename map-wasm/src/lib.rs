#![cfg_attr(all(not(test), target_arch = "wasm32"), no_std)]

#[cfg(all(not(test), target_arch = "wasm32"))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}

const MAP_WIDTH: usize = 720;
const MAP_HEIGHT: usize = 360;
const MAP_CELL_COUNT: usize = MAP_WIDTH * MAP_HEIGHT;

const MAX_RENDER_WIDTH: usize = 2_048;
const MAX_RENDER_HEIGHT: usize = 1_024;
const MAX_PIXEL_COUNT: usize = MAX_RENDER_WIDTH * MAX_RENDER_HEIGHT;
const MAX_PIXEL_BYTES: usize = MAX_PIXEL_COUNT * 4;

const WATER: u8 = 255;
const OUTSIDE: u8 = 254;
const LAND: u8 = 0;
const FOREST: u8 = 100;

const VIEW_FLAT: u32 = 0;
const VIEW_GLOBE: u32 = 1;
const PHASE_MASK: u32 = 65_535;
const PHASE_TURN: f32 = 65_536.0;
// 80 degrees in the same signed turn units used for yaw. Keeping ten degrees
// away from either pole prevents the globe controls from flipping over.
const MAX_PITCH_PHASE: i32 = 14_564;
const HALF_TURN_PHASE: u16 = 32_768;

const PI: f32 = core::f32::consts::PI;
const HALF_PI: f32 = core::f32::consts::FRAC_PI_2;
const TAU: f32 = core::f32::consts::TAU;

const WORLD_FOREST: &[u8; MAP_CELL_COUNT] = include_bytes!("../assets/world_forest_v1.bin");

// The module is deliberately single-threaded. Browser code calls render and
// forest_at sequentially on the main thread, so fixed buffers avoid an
// allocator, imported functions, and data-dependent memory growth. Pixels are
// u32-aligned so whole RGBA values can be written in one store.
static mut PIXELS: [u32; MAX_PIXEL_COUNT] = [0; MAX_PIXEL_COUNT];
static mut PIXEL_LENGTH: usize = 0;
static mut LAST_WIDTH: usize = 0;
static mut LAST_HEIGHT: usize = 0;
static mut LAST_VIEW: u32 = VIEW_FLAT;
static mut LAST_PHASE: u32 = 0;
static mut LAST_PITCH: i32 = 0;
static mut LAST_THEME: u32 = 0;

// Source-cell presentation classes collapse the repeated coastline probes in
// the hot render loop to one byte lookup. This is initialized once at runtime
// so it occupies zero-filled memory rather than doubling the checked-in atlas
// in the WebAssembly binary.
const CLASS_LAND: u8 = 0;
const CLASS_FOREST: u8 = 1;
const CLASS_WATER: u8 = 2;
const CLASS_COAST: u8 = 3;
static mut SOURCE_CLASSES: [u8; MAP_CELL_COUNT] = [0; MAP_CELL_COUNT];
static mut SOURCE_CLASSES_READY: bool = false;

// Geometry depends only on the canvas dimensions. Pose mapping depends on
// pitch but not yaw, so horizontal drag frames reuse it and only add the yaw
// phase before sampling the atlas. Each packed mapping stores source_y in the
// upper bits and a wrapped local-longitude phase in the low 16 bits.
static mut GLOBE_X: [f32; MAX_RENDER_WIDTH] = [0.0; MAX_RENDER_WIDTH];
static mut GLOBE_Y: [f32; MAX_RENDER_HEIGHT] = [0.0; MAX_RENDER_HEIGHT];
static mut GLOBE_ROW_START: [u16; MAX_RENDER_HEIGHT] = [0; MAX_RENDER_HEIGHT];
static mut GLOBE_ROW_END: [u16; MAX_RENDER_HEIGHT] = [0; MAX_RENDER_HEIGHT];
static mut GLOBE_MAPPING: [u32; MAX_PIXEL_COUNT] = [0; MAX_PIXEL_COUNT];
static mut GEOMETRY_WIDTH: usize = 0;
static mut GEOMETRY_HEIGHT: usize = 0;
static mut MAPPING_PITCH: i32 = i32::MIN;

#[derive(Clone, Copy)]
struct Rgb {
    red: u8,
    green: u8,
    blue: u8,
}

const LIGHT_WATER: Rgb = Rgb::new(184, 211, 213);
const LIGHT_LAND: Rgb = Rgb::new(221, 216, 197);
const LIGHT_FOREST: Rgb = Rgb::new(39, 100, 66);
const LIGHT_COAST: Rgb = Rgb::new(91, 110, 101);
const LIGHT_OUTSIDE: Rgb = Rgb::new(238, 240, 234);

const DARK_WATER: Rgb = Rgb::new(20, 45, 50);
const DARK_LAND: Rgb = Rgb::new(62, 61, 49);
const DARK_FOREST: Rgb = Rgb::new(74, 140, 111);
const DARK_COAST: Rgb = Rgb::new(132, 151, 142);
const DARK_OUTSIDE: Rgb = Rgb::new(12, 24, 24);

impl Rgb {
    const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }
}

fn flat_source_coordinates(x: usize, y: usize, width: usize, height: usize) -> (usize, usize) {
    (
        x.saturating_mul(MAP_WIDTH) / width,
        y.saturating_mul(MAP_HEIGHT) / height,
    )
}

// Minimax approximation of atan on [0, 1]. Its maximum error is below
// 0.0007 degrees, far smaller than one 0.5-degree source cell. Keeping this
// local avoids a libm import in the raw no_std WebAssembly module.
fn atan_unit(value: f32) -> f32 {
    let squared = value * value;
    value
        * (0.999_866
            + squared
                * (-0.330_299_5
                    + squared * (0.180_141 + squared * (-0.085_133 + squared * 0.020_835_1))))
}

// Scalar square root for positive projection coordinates. Three Newton steps
// from a bit-level exponent estimate converge to f32 precision and keep the
// no_std WebAssembly module free of libm imports.
fn sqrt_unit(value: f32) -> f32 {
    if value <= 0.0 {
        return 0.0;
    }

    let mut estimate = f32::from_bits((value.to_bits() >> 1) + 0x1fc0_0000);
    for _ in 0..3 {
        estimate = 0.5 * (estimate + value / estimate);
    }
    estimate
}

// atan2(y, x) for the front half of a sphere, where x is non-negative.
fn front_atan2(y: f32, x: f32) -> f32 {
    if y == 0.0 {
        return 0.0;
    }

    let magnitude = if y < 0.0 { -y } else { y };
    let angle = if x == 0.0 {
        HALF_PI
    } else if magnitude <= x {
        atan_unit(magnitude / x)
    } else {
        HALF_PI - atan_unit(x / magnitude)
    };

    if y < 0.0 {
        -angle
    } else {
        angle
    }
}

fn full_atan2(y: f32, x: f32) -> f32 {
    if x >= 0.0 {
        return front_atan2(y, x);
    }

    if y >= 0.0 {
        PI - front_atan2(y, -x)
    } else {
        -PI - front_atan2(y, -x)
    }
}

fn clamp_pitch(pitch: i32) -> i32 {
    pitch.clamp(-MAX_PITCH_PHASE, MAX_PITCH_PHASE)
}

// Pitch never leaves [-80 degrees, 80 degrees], where these Taylor series are
// accurate to substantially less than one source-map cell. They execute once
// for each new pitch rather than once per pixel.
fn pitch_sin_cos(pitch: i32) -> (f32, f32) {
    let radians = clamp_pitch(pitch) as f32 * (TAU / PHASE_TURN);
    let squared = radians * radians;
    let sine = radians
        * (1.0
            + squared
                * (-1.0 / 6.0
                    + squared
                        * (1.0 / 120.0
                            + squared * (-1.0 / 5_040.0 + squared * (1.0 / 362_880.0)))));
    let cosine = 1.0
        + squared
            * (-1.0 / 2.0
                + squared
                    * (1.0 / 24.0
                        + squared
                            * (-1.0 / 720.0
                                + squared * (1.0 / 40_320.0 + squared * (-1.0 / 3_628_800.0)))));
    (sine, cosine)
}

fn source_index(unit: f32, extent: usize) -> usize {
    if unit <= 0.0 {
        0
    } else if unit >= 1.0 {
        extent - 1
    } else {
        (unit * extent as f32) as usize
    }
}

fn globe_geometry(width: usize, height: usize) -> (f32, f32, f32) {
    let maximum_diameter = width.min(height);
    let diameter = if maximum_diameter >= 2 {
        (maximum_diameter * 94 / 100).max(2)
    } else {
        1
    };
    let disc_left = (width - diameter) / 2;
    let disc_top = (height - diameter) / 2;
    let radius = diameter as f32 * 0.5;
    (radius, disc_left as f32 + radius, disc_top as f32 + radius)
}

#[cfg(test)]
fn normalized_globe_point(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> Option<(f32, f32, f32)> {
    let (radius, center_x, center_y) = globe_geometry(width, height);
    let normalized_x = (x as f32 + 0.5 - center_x) / radius;
    let normalized_y = (center_y - (y as f32 + 0.5)) / radius;
    let distance_squared = normalized_x * normalized_x + normalized_y * normalized_y;
    if distance_squared > 1.0 {
        return None;
    }
    Some((
        normalized_x,
        normalized_y,
        sqrt_unit(1.0 - distance_squared),
    ))
}

fn local_longitude_phase(longitude: f32) -> u16 {
    (longitude * (PHASE_TURN / TAU)) as i32 as u16
}

fn source_x_for_phase(local_phase: u16, yaw: u32) -> usize {
    let atlas_phase = local_phase
        .wrapping_add(yaw as u16)
        .wrapping_add(HALF_TURN_PHASE);
    (atlas_phase as usize * MAP_WIDTH) >> 16
}

fn pose_mapping(
    normalized_x: f32,
    normalized_y: f32,
    front: f32,
    pitch_sine: f32,
    pitch_cosine: f32,
) -> (u16, usize) {
    let world_y = (normalized_y * pitch_cosine + front * pitch_sine).clamp(-1.0, 1.0);
    let horizontal_front = front * pitch_cosine - normalized_y * pitch_sine;
    let latitude_front = sqrt_unit(1.0 - world_y * world_y);
    let latitude = front_atan2(world_y, latitude_front);
    let local_longitude = full_atan2(normalized_x, horizontal_front);
    (
        local_longitude_phase(local_longitude),
        source_index((HALF_PI - latitude) / PI, MAP_HEIGHT),
    )
}

#[cfg(test)]
fn globe_source_coordinates(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    phase: u32,
) -> Option<(usize, usize)> {
    globe_pose_source_coordinates(x, y, width, height, phase, 0)
}

#[cfg(test)]
fn globe_pose_source_coordinates(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    yaw: u32,
    pitch: i32,
) -> Option<(usize, usize)> {
    let (normalized_x, normalized_y, front) = normalized_globe_point(x, y, width, height)?;
    let (pitch_sine, pitch_cosine) = pitch_sin_cos(pitch);
    let (local_phase, source_y) =
        pose_mapping(normalized_x, normalized_y, front, pitch_sine, pitch_cosine);
    Some((source_x_for_phase(local_phase, yaw), source_y))
}

fn source_value(source_x: usize, source_y: usize) -> u8 {
    WORLD_FOREST[source_y * MAP_WIDTH + source_x]
}

fn is_coast(source_x: usize, source_y: usize) -> bool {
    if source_value(source_x, source_y) != WATER {
        return false;
    }

    (source_x > 0 && source_value(source_x - 1, source_y) != WATER)
        || (source_x + 1 < MAP_WIDTH && source_value(source_x + 1, source_y) != WATER)
        || (source_y > 0 && source_value(source_x, source_y - 1) != WATER)
        || (source_y + 1 < MAP_HEIGHT && source_value(source_x, source_y + 1) != WATER)
}

fn ensure_source_classes() {
    unsafe {
        if SOURCE_CLASSES_READY {
            return;
        }
        let classes = core::ptr::addr_of_mut!(SOURCE_CLASSES).cast::<u8>();
        for source_y in 0..MAP_HEIGHT {
            for source_x in 0..MAP_WIDTH {
                let value = source_value(source_x, source_y);
                let class = match value {
                    FOREST => CLASS_FOREST,
                    LAND => CLASS_LAND,
                    WATER if is_coast(source_x, source_y) => CLASS_COAST,
                    _ => CLASS_WATER,
                };
                classes.add(source_y * MAP_WIDTH + source_x).write(class);
            }
        }
        SOURCE_CLASSES_READY = true;
    }
}

fn source_class(source_x: usize, source_y: usize) -> u8 {
    unsafe {
        core::ptr::addr_of!(SOURCE_CLASSES)
            .cast::<u8>()
            .add(source_y * MAP_WIDTH + source_x)
            .read()
    }
}

fn packed_color(color: Rgb) -> u32 {
    u32::from_le_bytes([color.red, color.green, color.blue, 255])
}

fn packed_class_color(class: u8, dark: bool) -> u32 {
    packed_color(match (dark, class) {
        (false, CLASS_LAND) => LIGHT_LAND,
        (false, CLASS_FOREST) => LIGHT_FOREST,
        (false, CLASS_COAST) => LIGHT_COAST,
        (false, _) => LIGHT_WATER,
        (true, CLASS_LAND) => DARK_LAND,
        (true, CLASS_FOREST) => DARK_FOREST,
        (true, CLASS_COAST) => DARK_COAST,
        (true, _) => DARK_WATER,
    })
}

fn class_colors(dark: bool) -> [u32; 4] {
    [
        packed_class_color(CLASS_LAND, dark),
        packed_class_color(CLASS_FOREST, dark),
        packed_class_color(CLASS_WATER, dark),
        packed_class_color(CLASS_COAST, dark),
    ]
}

fn rebuild_globe_geometry(width: usize, height: usize) {
    unsafe {
        if GEOMETRY_WIDTH == width && GEOMETRY_HEIGHT == height {
            return;
        }

        let (radius, center_x, center_y) = globe_geometry(width, height);
        let globe_x = core::ptr::addr_of_mut!(GLOBE_X).cast::<f32>();
        let globe_y = core::ptr::addr_of_mut!(GLOBE_Y).cast::<f32>();
        let row_start = core::ptr::addr_of_mut!(GLOBE_ROW_START).cast::<u16>();
        let row_end = core::ptr::addr_of_mut!(GLOBE_ROW_END).cast::<u16>();

        for x in 0..width {
            globe_x.add(x).write((x as f32 + 0.5 - center_x) / radius);
        }
        for y in 0..height {
            let normalized_y = (center_y - (y as f32 + 0.5)) / radius;
            globe_y.add(y).write(normalized_y);
            let mut first = width;
            let mut end = 0;
            for x in 0..width {
                let normalized_x = globe_x.add(x).read();
                if normalized_x * normalized_x + normalized_y * normalized_y <= 1.0 {
                    first = first.min(x);
                    end = x + 1;
                }
            }
            if first == width {
                first = 0;
                end = 0;
            }
            row_start.add(y).write(first as u16);
            row_end.add(y).write(end as u16);
        }

        GEOMETRY_WIDTH = width;
        GEOMETRY_HEIGHT = height;
        MAPPING_PITCH = i32::MIN;
    }
}

fn rebuild_globe_mapping(width: usize, height: usize, pitch: i32) {
    rebuild_globe_geometry(width, height);
    let pitch = clamp_pitch(pitch);
    unsafe {
        if MAPPING_PITCH == pitch {
            return;
        }

        let (pitch_sine, pitch_cosine) = pitch_sin_cos(pitch);
        let globe_x = core::ptr::addr_of!(GLOBE_X).cast::<f32>();
        let globe_y = core::ptr::addr_of!(GLOBE_Y).cast::<f32>();
        let row_start = core::ptr::addr_of!(GLOBE_ROW_START).cast::<u16>();
        let row_end = core::ptr::addr_of!(GLOBE_ROW_END).cast::<u16>();
        let mapping = core::ptr::addr_of_mut!(GLOBE_MAPPING).cast::<u32>();

        for y in 0..height {
            let normalized_y = globe_y.add(y).read();
            let start = row_start.add(y).read() as usize;
            let end = row_end.add(y).read() as usize;
            for x in start..end {
                let normalized_x = globe_x.add(x).read();
                let front =
                    sqrt_unit(1.0 - normalized_x * normalized_x - normalized_y * normalized_y);
                let (local_phase, source_y) =
                    pose_mapping(normalized_x, normalized_y, front, pitch_sine, pitch_cosine);
                mapping
                    .add(y * width + x)
                    .write((source_y as u32) << 16 | local_phase as u32);
            }
        }
        MAPPING_PITCH = pitch;
    }
}

fn cached_globe_source_coordinates(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    yaw: u32,
    pitch: i32,
) -> Option<(usize, usize)> {
    rebuild_globe_mapping(width, height, pitch);
    unsafe {
        let start = core::ptr::addr_of!(GLOBE_ROW_START)
            .cast::<u16>()
            .add(y)
            .read() as usize;
        let end = core::ptr::addr_of!(GLOBE_ROW_END)
            .cast::<u16>()
            .add(y)
            .read() as usize;
        if x < start || x >= end {
            return None;
        }
        let packed = core::ptr::addr_of!(GLOBE_MAPPING)
            .cast::<u32>()
            .add(y * width + x)
            .read();
        Some((
            source_x_for_phase(packed as u16, yaw),
            (packed >> 16) as usize,
        ))
    }
}

fn fill_pixels(pixel_count: usize, color: u32) {
    unsafe {
        core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(PIXELS).cast::<u32>(), pixel_count)
            .fill(color);
    }
}

fn render_flat_frame(width: usize, height: usize, dark: bool) {
    let output = core::ptr::addr_of_mut!(PIXELS).cast::<u32>();
    let colors = class_colors(dark);
    for y in 0..height {
        let source_y = y * MAP_HEIGHT / height;
        for x in 0..width {
            let source_x = x * MAP_WIDTH / width;
            let class = source_class(source_x, source_y);
            unsafe {
                output.add(y * width + x).write(colors[class as usize]);
            }
        }
    }
}

fn render_globe_frame(width: usize, height: usize, dark: bool, yaw: u32, pitch: i32) {
    rebuild_globe_mapping(width, height, pitch);
    let output = core::ptr::addr_of_mut!(PIXELS).cast::<u32>();
    let colors = class_colors(dark);
    let normalized_theme = dark as u32;
    let outside_is_current = unsafe {
        LAST_VIEW == VIEW_GLOBE
            && LAST_WIDTH == width
            && LAST_HEIGHT == height
            && LAST_THEME == normalized_theme
            && PIXEL_LENGTH == width * height * 4
    };
    if !outside_is_current {
        fill_pixels(
            width * height,
            packed_color(if dark { DARK_OUTSIDE } else { LIGHT_OUTSIDE }),
        );
    }

    unsafe {
        let row_start = core::ptr::addr_of!(GLOBE_ROW_START).cast::<u16>();
        let row_end = core::ptr::addr_of!(GLOBE_ROW_END).cast::<u16>();
        let mapping = core::ptr::addr_of!(GLOBE_MAPPING).cast::<u32>();
        for y in 0..height {
            let start = row_start.add(y).read() as usize;
            let end = row_end.add(y).read() as usize;
            for x in start..end {
                let packed = mapping.add(y * width + x).read();
                let source_x = source_x_for_phase(packed as u16, yaw);
                let source_y = (packed >> 16) as usize;
                let class = source_class(source_x, source_y);
                output.add(y * width + x).write(colors[class as usize]);
            }
        }
    }
}

fn clear_render_state() {
    unsafe {
        PIXEL_LENGTH = 0;
        LAST_WIDTH = 0;
        LAST_HEIGHT = 0;
        LAST_VIEW = VIEW_FLAT;
        LAST_PHASE = 0;
        LAST_PITCH = 0;
        LAST_THEME = 0;
    }
}

/// Render a fixed whole-world equirectangular view into the exported memory.
///
/// `theme == 0` selects the light palette; every other value selects dark.
/// Returns 1 on success and 0 when the requested dimensions are invalid.
#[no_mangle]
pub extern "C" fn render(width: u32, height: u32, theme: u32) -> u32 {
    render_projection(width, height, theme, VIEW_FLAT, 0, 0)
}

/// Render either the flat world (`view == 0`) or a Greenwich-centred
/// orthographic globe (`view == 1`).
#[no_mangle]
pub extern "C" fn render_view(width: u32, height: u32, theme: u32, view: u32) -> u32 {
    render_projection(width, height, theme, view, 0, 0)
}

/// Render one orthographic globe. The low 16 bits of `phase` represent a full
/// eastward turn of the centre longitude; phase zero is Greenwich.
#[no_mangle]
pub extern "C" fn render_globe(width: u32, height: u32, theme: u32, phase: u32) -> u32 {
    render_projection(width, height, theme, VIEW_GLOBE, phase, 0)
}

/// Render an orthographic globe at an explicit two-axis pose. `yaw` uses the
/// same modulo-65536 eastward turn as `render_globe`. `pitch` is signed in the
/// same units, positive northward, and is clamped to +/-80 degrees.
#[no_mangle]
pub extern "C" fn render_globe_pose(
    width: u32,
    height: u32,
    theme: u32,
    yaw: u32,
    pitch: i32,
) -> u32 {
    render_projection(width, height, theme, VIEW_GLOBE, yaw, pitch)
}

fn render_projection(
    width: u32,
    height: u32,
    theme: u32,
    view: u32,
    phase: u32,
    pitch: i32,
) -> u32 {
    let width = width as usize;
    let height = height as usize;
    let Some(pixel_count) = width.checked_mul(height) else {
        clear_render_state();
        return 0;
    };
    let Some(pixel_length) = pixel_count.checked_mul(4) else {
        clear_render_state();
        return 0;
    };

    if width < 2
        || height < 1
        || width > MAX_RENDER_WIDTH
        || height > MAX_RENDER_HEIGHT
        || pixel_length > MAX_PIXEL_BYTES
        || !matches!(view, VIEW_FLAT | VIEW_GLOBE)
    {
        clear_render_state();
        return 0;
    }

    let dark = theme != 0;
    let normalized_theme = dark as u32;
    let phase = phase & PHASE_MASK;
    let pitch = if view == VIEW_GLOBE {
        clamp_pitch(pitch)
    } else {
        0
    };
    let already_current = unsafe {
        LAST_WIDTH == width
            && LAST_HEIGHT == height
            && LAST_VIEW == view
            && LAST_PHASE == phase
            && LAST_PITCH == pitch
            && LAST_THEME == normalized_theme
            && PIXEL_LENGTH == pixel_length
    };
    if already_current {
        return 1;
    }

    ensure_source_classes();
    match view {
        VIEW_FLAT => render_flat_frame(width, height, dark),
        VIEW_GLOBE => render_globe_frame(width, height, dark, phase, pitch),
        _ => unreachable!(),
    }

    unsafe {
        PIXEL_LENGTH = pixel_length;
        LAST_WIDTH = width;
        LAST_HEIGHT = height;
        LAST_VIEW = view;
        LAST_PHASE = phase;
        LAST_PITCH = pitch;
        LAST_THEME = normalized_theme;
    }
    1
}

#[no_mangle]
pub extern "C" fn pixel_ptr() -> u32 {
    core::ptr::addr_of!(PIXELS).cast::<u8>() as usize as u32
}

#[no_mangle]
pub extern "C" fn pixel_len() -> u32 {
    unsafe { PIXEL_LENGTH as u32 }
}

/// Return the categorical state at a pixel in the most recent render.
///
/// Values are 100 for estimated forest presence, 0 for land where forest is
/// not shown, 254 outside a globe disc, and 255 for water, no estimate, or
/// invalid coordinates.
#[no_mangle]
pub extern "C" fn forest_at(x: u32, y: u32) -> u32 {
    let (width, height, view, phase, pitch) =
        unsafe { (LAST_WIDTH, LAST_HEIGHT, LAST_VIEW, LAST_PHASE, LAST_PITCH) };
    let x = x as usize;
    let y = y as usize;
    if width == 0 || height == 0 || x >= width || y >= height {
        return WATER as u32;
    }
    let coordinates = match view {
        VIEW_FLAT => Some(flat_source_coordinates(x, y, width, height)),
        VIEW_GLOBE => cached_globe_source_coordinates(x, y, width, height, phase, pitch),
        _ => None,
    };
    match coordinates {
        Some((source_x, source_y)) => source_value(source_x, source_y) as u32,
        None => OUTSIDE as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static RENDER_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn pinned_asset_has_only_documented_states() {
        let mut water = 0;
        let mut land = 0;
        let mut forest = 0;
        for value in WORLD_FOREST {
            match *value {
                WATER => water += 1,
                LAND => land += 1,
                FOREST => forest += 1,
                other => panic!("unexpected map state {other}"),
            }
        }
        assert_eq!((water, land, forest), (169_667, 70_650, 18_883));
    }

    #[test]
    fn browser_loader_uses_the_same_render_status_contract() {
        let loader = include_str!("../../public-app/map/map-loader.js");
        assert!(loader.contains("wasm.render_globe_pose("));
        assert!(loader.contains("wasm.render_globe("));
        assert!(loader.contains("wasm.render_view("));
        assert!(loader.contains("wasm.render("));
        assert!(loader.contains("if (renderResult !== 1)"));
    }

    #[test]
    fn globe_inverse_projection_tracks_phase_and_keeps_the_rim_visible() {
        assert_eq!(
            globe_source_coordinates(359, 179, MAP_WIDTH, MAP_HEIGHT, 0),
            Some((359, 179))
        );
        assert_eq!(
            globe_source_coordinates(359, 179, MAP_WIDTH, MAP_HEIGHT, 16_384),
            Some((539, 179))
        );
        assert_eq!(
            globe_source_coordinates(359, 179, MAP_WIDTH, MAP_HEIGHT, 65_536),
            globe_source_coordinates(359, 179, MAP_WIDTH, MAP_HEIGHT, 0)
        );
        assert_eq!(
            globe_source_coordinates(359, 179, MAP_WIDTH, MAP_HEIGHT, 81_920),
            globe_source_coordinates(359, 179, MAP_WIDTH, MAP_HEIGHT, 16_384)
        );

        // A quiet three-percent margin surrounds the centred globe, while its
        // cardinal rim pixels remain part of the spherical surface.
        assert!(globe_source_coordinates(191, 179, MAP_WIDTH, MAP_HEIGHT, 0).is_some());
        assert!(globe_source_coordinates(528, 179, MAP_WIDTH, MAP_HEIGHT, 0).is_some());
        assert!(globe_source_coordinates(359, 11, MAP_WIDTH, MAP_HEIGHT, 0).is_some());
        assert!(globe_source_coordinates(359, 348, MAP_WIDTH, MAP_HEIGHT, 0).is_some());
        assert_eq!(
            globe_source_coordinates(190, 179, MAP_WIDTH, MAP_HEIGHT, 0),
            None
        );
        assert_eq!(
            globe_source_coordinates(529, 179, MAP_WIDTH, MAP_HEIGHT, 0),
            None
        );
        assert_eq!(
            globe_source_coordinates(0, 0, MAP_WIDTH, MAP_HEIGHT, 0),
            None
        );
        assert_eq!(
            globe_source_coordinates(719, 359, MAP_WIDTH, MAP_HEIGHT, 0),
            None
        );

        let western_sample =
            globe_source_coordinates(280, 100, MAP_WIDTH, MAP_HEIGHT, 0).expect("west sample");
        let eastern_sample =
            globe_source_coordinates(439, 100, MAP_WIDTH, MAP_HEIGHT, 0).expect("east sample");
        assert_eq!(western_sample.1, eastern_sample.1);
        assert!((122..=124).contains(&western_sample.1));
    }

    #[test]
    fn coastline_is_structural_and_never_masks_land_or_forest() {
        let mut coast_cells = 0;
        for source_y in 0..MAP_HEIGHT {
            for source_x in 0..MAP_WIDTH {
                if is_coast(source_x, source_y) {
                    coast_cells += 1;
                    assert_eq!(source_value(source_x, source_y), WATER);
                }
            }
        }
        assert!(coast_cells > 0);
    }

    #[test]
    fn invalid_dimensions_fail_closed() {
        let _render_guard = RENDER_LOCK.lock().expect("renderer test lock");
        assert_eq!(render(0, 360, 0), 0);
        assert_eq!(pixel_len(), 0);
        assert_eq!(forest_at(0, 0), WATER as u32);

        assert_eq!(render((MAX_RENDER_WIDTH + 1) as u32, 1, 0), 0);
        assert_eq!(pixel_len(), 0);

        assert_eq!(render_view(720, 360, 0, 2), 0);
        assert_eq!(pixel_len(), 0);
        assert_eq!(forest_at(0, 0), WATER as u32);

        assert_eq!(render_globe_pose(720, 0, 0, 0, 0), 0);
        assert_eq!(pixel_len(), 0);
    }

    #[test]
    fn globe_render_tracks_phase_and_reports_outside_pixels() {
        let _render_guard = RENDER_LOCK.lock().expect("renderer test lock");
        assert_eq!(render_view(720, 360, 0, VIEW_GLOBE), 1);
        assert_eq!(pixel_len(), 720 * 360 * 4);
        assert_eq!(forest_at(0, 0), OUTSIDE as u32);
        assert_eq!(forest_at(719, 359), OUTSIDE as u32);
        assert_eq!(forest_at(720, 0), WATER as u32);

        for (x, y) in [(359, 179), (360, 179), (191, 179), (528, 179)] {
            let (source_x, source_y) =
                globe_source_coordinates(x, y, 720, 360, 0).expect("globe pixel");
            assert_eq!(
                forest_at(x as u32, y as u32),
                source_value(source_x, source_y) as u32
            );
        }

        let outside_pixel = unsafe {
            let output = core::ptr::addr_of!(PIXELS).cast::<u8>();
            [
                output.read(),
                output.add(1).read(),
                output.add(2).read(),
                output.add(3).read(),
            ]
        };
        assert_eq!(
            outside_pixel,
            [
                LIGHT_OUTSIDE.red,
                LIGHT_OUTSIDE.green,
                LIGHT_OUTSIDE.blue,
                255,
            ]
        );

        let mut outside = 0;
        let mut water = 0;
        let mut land = 0;
        let mut forest = 0;
        for y in 0..360 {
            for x in 0..720 {
                match forest_at(x, y) as u8 {
                    OUTSIDE => outside += 1,
                    WATER => water += 1,
                    LAND => land += 1,
                    FOREST => forest += 1,
                    state => panic!("unexpected projected state {state}"),
                }
            }
        }
        assert!(outside > 0);
        assert!(water > 0);
        assert!(land > 0);
        assert!(forest > 0);

        let mut phase_probe = None;
        'rows: for y in 0..360 {
            for x in 0..720 {
                let Some(phase_zero) = globe_source_coordinates(x, y, 720, 360, 0) else {
                    continue;
                };
                let Some(quarter_turn) = globe_source_coordinates(x, y, 720, 360, 16_384) else {
                    continue;
                };
                let zero_value = source_value(phase_zero.0, phase_zero.1);
                let quarter_value = source_value(quarter_turn.0, quarter_turn.1);
                if zero_value != quarter_value {
                    phase_probe = Some((x, y, zero_value, quarter_value));
                    break 'rows;
                }
            }
        }
        let (probe_x, probe_y, zero_value, quarter_value) =
            phase_probe.expect("phase-sensitive forest sample");

        assert_eq!(render_globe(720, 360, 0, 16_384), 1);
        assert_eq!(
            forest_at(probe_x as u32, probe_y as u32),
            quarter_value as u32
        );
        assert_ne!(forest_at(probe_x as u32, probe_y as u32), zero_value as u32);

        // Phase is modulo one unsigned 16-bit turn.
        assert_eq!(render_globe(720, 360, 0, 65_536), 1);
        let phase_wrapped = forest_at(359, 179);
        assert_eq!(render_globe(720, 360, 0, 0), 1);
        assert_eq!(forest_at(359, 179), phase_wrapped);

        // The generic view entry point always selects phase-zero Greenwich.
        assert_eq!(render_globe(720, 360, 0, 16_384), 1);
        assert_eq!(render_view(720, 360, 0, VIEW_GLOBE), 1);
        let (source_x, source_y) =
            globe_source_coordinates(359, 179, 720, 360, 0).expect("Greenwich centre");
        assert_eq!(forest_at(359, 179), source_value(source_x, source_y) as u32);

        // The legacy entry point must always restore the flat interpretation.
        assert_eq!(render(720, 360, 0), 1);
        assert_eq!(forest_at(0, 0), source_value(0, 0) as u32);
    }

    #[test]
    fn globe_pose_tracks_pitch_clamps_safely_and_restores_legacy_pose() {
        let _render_guard = RENDER_LOCK.lock().expect("renderer test lock");
        let (center_x, center_y) = (359, 179);
        let zero = globe_pose_source_coordinates(center_x, center_y, 720, 360, 0, 0)
            .expect("zero-pitch center");
        let north = globe_pose_source_coordinates(center_x, center_y, 720, 360, 0, 8_192)
            .expect("north-pitch center");
        let south = globe_pose_source_coordinates(center_x, center_y, 720, 360, 0, -8_192)
            .expect("south-pitch center");
        assert!(north.1 < zero.1);
        assert!(south.1 > zero.1);

        assert_eq!(render_globe_pose(720, 360, 0, 4_096, 8_192), 1);
        for (x, y) in [(center_x, center_y), (280, 100), (439, 240)] {
            let (source_x, source_y) = globe_pose_source_coordinates(x, y, 720, 360, 4_096, 8_192)
                .expect("pitched globe pixel");
            assert_eq!(
                forest_at(x as u32, y as u32),
                source_value(source_x, source_y) as u32
            );
        }
        assert_eq!(unsafe { LAST_PITCH }, 8_192);

        assert_eq!(render_globe_pose(720, 360, 0, 0, MAX_PITCH_PHASE), 1);
        let clamped_samples = [
            forest_at(359, 179),
            forest_at(280, 100),
            forest_at(439, 240),
        ];
        assert_eq!(render_globe_pose(720, 360, 0, 0, i32::MAX), 1);
        assert_eq!(unsafe { LAST_PITCH }, MAX_PITCH_PHASE);
        assert_eq!(
            clamped_samples,
            [
                forest_at(359, 179),
                forest_at(280, 100),
                forest_at(439, 240),
            ]
        );

        // The original ABI remains a zero-pitch compatibility wrapper.
        assert_eq!(render_globe(720, 360, 0, 0), 1);
        assert_eq!(unsafe { LAST_PITCH }, 0);
        let (source_x, source_y) =
            globe_source_coordinates(center_x, center_y, 720, 360, 0).expect("legacy globe");
        assert_eq!(
            forest_at(center_x as u32, center_y as u32),
            source_value(source_x, source_y) as u32
        );
    }

    #[test]
    fn render_preserves_categories_and_writes_opaque_pixels() {
        let _render_guard = RENDER_LOCK.lock().expect("renderer test lock");
        assert_eq!(render(MAP_WIDTH as u32, MAP_HEIGHT as u32, 0), 1);
        assert_eq!(pixel_len() as usize, MAP_CELL_COUNT * 4);

        let forest_index = WORLD_FOREST
            .iter()
            .position(|value| *value == FOREST)
            .expect("forest cell");
        let land_index = WORLD_FOREST
            .iter()
            .position(|value| *value == LAND)
            .expect("land cell");
        let water_index = WORLD_FOREST
            .iter()
            .position(|value| *value == WATER)
            .expect("water cell");

        for (index, expected) in [
            (forest_index, FOREST),
            (land_index, LAND),
            (water_index, WATER),
        ] {
            let x = index % MAP_WIDTH;
            let y = index / MAP_WIDTH;
            assert_eq!(forest_at(x as u32, y as u32), expected as u32);
            let alpha_offset = index * 4 + 3;
            let alpha = unsafe {
                core::ptr::addr_of!(PIXELS)
                    .cast::<u8>()
                    .add(alpha_offset)
                    .read()
            };
            assert_eq!(alpha, 255);
        }
    }

    #[test]
    fn theme_changes_color_without_changing_data() {
        let _render_guard = RENDER_LOCK.lock().expect("renderer test lock");
        let index = WORLD_FOREST
            .iter()
            .position(|value| *value == FOREST)
            .expect("forest cell");
        let x = index % MAP_WIDTH;
        let y = index / MAP_WIDTH;

        assert_eq!(render(MAP_WIDTH as u32, MAP_HEIGHT as u32, 0), 1);
        let offset = index * 4;
        let light_red = unsafe { core::ptr::addr_of!(PIXELS).cast::<u8>().add(offset).read() };

        assert_eq!(render(MAP_WIDTH as u32, MAP_HEIGHT as u32, 1), 1);
        let dark_red = unsafe { core::ptr::addr_of!(PIXELS).cast::<u8>().add(offset).read() };
        assert_ne!(light_red, dark_red);
        assert_eq!(forest_at(x as u32, y as u32), FOREST as u32);
    }
}
