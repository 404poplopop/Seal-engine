//! The Seal mark from the repository logo.

const LOGO_PNG: &[u8] = include_bytes!("../../../assets/logo.png");

/// Window icon. Pixels are straight RGBA, one byte per channel.
pub fn app_icon() -> egui::IconData {
    let (rgba, width, height) = decode_logo();
    egui::IconData { rgba, width, height }
}

/// Draw the repository logo. The toolkit caches it by name.
/// Faint centered mark for an empty dock region.
pub fn paint_dock_watermark(ui: &mut egui::Ui, rect: egui::Rect) {
    let (rgba, width, height) = decode_logo();
    if width < 2 || height < 2 || rgba.len() < width as usize * height as usize * 4 {
        return;
    }
    let side = rect.width().min(rect.height()) * 0.32;
    let side = side.clamp(72.0, 280.0);
    let logo_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(side, side));
    let image = egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &rgba);
    let texture = ui.ctx().load_texture("seal-logo-watermark", image, egui::TextureOptions::LINEAR);
    let tint = egui::Color32::from_white_alpha((255.0 * 0.06) as u8);
    ui.painter().image(
        texture.id(),
        logo_rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        tint,
    );
}

pub fn show_logo(ui: &mut egui::Ui, size: f32) {
    let (rgba, width, height) = decode_logo();
    if width < 2 || height < 2 || rgba.len() < width as usize * height as usize * 4 {
        return;
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &rgba);
    let texture = ui.ctx().load_texture("seal-logo", image, egui::TextureOptions::LINEAR);
    ui.add(egui::Image::from_texture(&texture).fit_to_exact_size(egui::vec2(size, size)));
}

fn decode_logo() -> (Vec<u8>, u32, u32) {
    let fallback = (vec![0, 0, 0, 0], 1, 1);
    let decoder = png::Decoder::new(std::io::Cursor::new(LOGO_PNG));
    let Ok(mut reader) = decoder.read_info() else {
        return fallback;
    };
    let color = reader.info().color_type;
    let wide = reader.info().bit_depth == png::BitDepth::Sixteen;
    let Some(size) = reader.output_buffer_size() else {
        return fallback;
    };
    let mut raw = vec![0; size];
    let Ok(frame) = reader.next_frame(&mut raw) else {
        return fallback;
    };
    let packed = if wide {
        raw[..frame.buffer_size()].as_chunks::<2>().0.iter().map(|pair| pair[0]).collect()
    } else {
        raw[..frame.buffer_size()].to_vec()
    };
    let rgba = match color {
        png::ColorType::Rgba => packed,
        png::ColorType::Rgb => packed.chunks(3).flat_map(|px| [px[0], px[1], px[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => {
            packed.chunks(2).flat_map(|px| [px[0], px[0], px[0], px[1]]).collect()
        }
        png::ColorType::Grayscale => packed.into_iter().flat_map(|px| [px, px, px, 255]).collect(),
        _ => return fallback,
    };
    (rgba, frame.width, frame.height)
}
