use std::{env, fs, io, path::Path};

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    if let Err(error) = add_windows_resources() {
        panic!("Unable to embed the BOREAL Windows icon: {error}");
    }
}

fn add_windows_resources() -> Result<(), Box<dyn std::error::Error>> {
    let output = env::var_os("OUT_DIR").ok_or("OUT_DIR is unavailable")?;
    let icon_path = Path::new(&output).join("boreal.ico");
    fs::write(&icon_path, windows_icon()?)?;

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(
        icon_path
            .to_str()
            .ok_or("The generated Windows icon path is not UTF-8")?,
    );
    resource.set("ProductName", "BOREAL");
    resource.set("FileDescription", "BOREAL Google Drive Organizer");
    resource.compile()?;
    Ok(())
}

/// Encode the BOREAL folder/link mark as a 32-bit ICO/DIB image.
fn windows_icon() -> io::Result<Vec<u8>> {
    const SIZE: u32 = 32;
    let rgba = boreal_icon_rgba(SIZE);
    let xor_size = SIZE * SIZE * 4;
    let and_stride = SIZE.div_ceil(32) * 4;
    let and_size = and_stride * SIZE;
    let image_size = 40 + xor_size + and_size;
    let mut icon = Vec::with_capacity((22 + image_size) as usize);

    icon.extend_from_slice(&0_u16.to_le_bytes());
    icon.extend_from_slice(&1_u16.to_le_bytes());
    icon.extend_from_slice(&1_u16.to_le_bytes());
    icon.extend_from_slice(&[SIZE as u8, SIZE as u8, 0, 0]);
    icon.extend_from_slice(&1_u16.to_le_bytes());
    icon.extend_from_slice(&32_u16.to_le_bytes());
    icon.extend_from_slice(&image_size.to_le_bytes());
    icon.extend_from_slice(&22_u32.to_le_bytes());

    icon.extend_from_slice(&40_u32.to_le_bytes());
    icon.extend_from_slice(&(SIZE as i32).to_le_bytes());
    icon.extend_from_slice(&((SIZE * 2) as i32).to_le_bytes());
    icon.extend_from_slice(&1_u16.to_le_bytes());
    icon.extend_from_slice(&32_u16.to_le_bytes());
    icon.extend_from_slice(&0_u32.to_le_bytes());
    icon.extend_from_slice(&xor_size.to_le_bytes());
    icon.extend_from_slice(&0_i32.to_le_bytes());
    icon.extend_from_slice(&0_i32.to_le_bytes());
    icon.extend_from_slice(&0_u32.to_le_bytes());
    icon.extend_from_slice(&0_u32.to_le_bytes());

    for y in (0..SIZE).rev() {
        for x in 0..SIZE {
            let offset = ((y * SIZE + x) * 4) as usize;
            icon.extend_from_slice(&[
                rgba[offset + 2],
                rgba[offset + 1],
                rgba[offset],
                rgba[offset + 3],
            ]);
        }
    }
    icon.resize((22 + image_size) as usize, 0);
    Ok(icon)
}

fn boreal_icon_rgba(size: u32) -> Vec<u8> {
    let mut pixels = vec![0_u8; (size * size * 4) as usize];
    let scale = size as f32 / 32.0;
    let inside = |x: u32, y: u32, left: f32, top: f32, right: f32, bottom: f32| {
        (x as f32) >= left * scale
            && (x as f32) < right * scale
            && (y as f32) >= top * scale
            && (y as f32) < bottom * scale
    };
    let set = |pixels: &mut [u8], x: u32, y: u32, color: [u8; 4]| {
        let offset = ((y * size + x) * 4) as usize;
        pixels[offset..offset + 4].copy_from_slice(&color);
    };
    for y in 0..size {
        for x in 0..size {
            if inside(x, y, 3.0, 8.0, 29.0, 26.0) || inside(x, y, 5.0, 5.0, 15.0, 11.0) {
                set(&mut pixels, x, y, [13, 110, 253, 255]);
            }
            if inside(x, y, 5.0, 11.0, 27.0, 24.0) {
                set(&mut pixels, x, y, [25, 135, 250, 255]);
            }
        }
    }
    for step in 0..12_u32 {
        let x = ((9.0 + step as f32) * scale) as u32;
        let y = ((21.0 - step as f32 * 0.65) * scale) as u32;
        let thickness = scale.max(1.0) as u32;
        for offset in 0..thickness {
            if x < size && y + offset < size {
                set(&mut pixels, x, y + offset, [255, 255, 255, 255]);
            }
        }
    }
    for step in 0..6_u32 {
        let x = ((18.0 + step as f32) * scale) as u32;
        let upper = ((13.0 + step as f32) * scale) as u32;
        let lower = ((13.0 + (5 - step) as f32) * scale) as u32;
        if x < size && upper < size {
            set(&mut pixels, x, upper, [255, 255, 255, 255]);
        }
        if x < size && lower < size {
            set(&mut pixels, x, lower, [255, 255, 255, 255]);
        }
    }
    pixels
}
