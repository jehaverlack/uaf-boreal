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

/// Encode the Bootstrap-style BOREAL mark as a 32-bit ICO/DIB image.
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
    let set = |pixels: &mut [u8], x: u32, y: u32, color: [u8; 4]| {
        let offset = ((y * size + x) * 4) as usize;
        pixels[offset..offset + 4].copy_from_slice(&color);
    };
    for y in 0..size {
        for x in 0..size {
            let px = (x as f32 + 0.5) * 32.0 / size as f32;
            let py = (y as f32 + 0.5) * 32.0 / size as f32;
            let dx = (7.0 - px).max(0.0).max(px - 25.0);
            let dy = (7.0 - py).max(0.0).max(py - 25.0);
            if dx * dx + dy * dy <= 25.0 {
                set(&mut pixels, x, y, [0, 132, 193, 255]);
                let stem = (9.5..13.0).contains(&px) && (7.0..25.0).contains(&py);
                let upper_outer = ((px - 15.0) / 7.0).powi(2) + ((py - 11.5) / 5.0).powi(2) <= 1.0;
                let upper_inner = ((px - 14.5) / 3.0).powi(2) + ((py - 11.5) / 2.1).powi(2) <= 1.0;
                let lower_outer = ((px - 15.0) / 7.5).powi(2) + ((py - 20.0) / 5.5).powi(2) <= 1.0;
                let lower_inner = ((px - 14.5) / 3.2).powi(2) + ((py - 20.0) / 2.4).powi(2) <= 1.0;
                if stem
                    || (px >= 11.0 && upper_outer && !upper_inner)
                    || (px >= 11.0 && lower_outer && !lower_inner)
                {
                    set(&mut pixels, x, y, [255, 255, 255, 255]);
                }
            }
        }
    }
    pixels
}
