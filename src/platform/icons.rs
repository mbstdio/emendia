//! Embedded application logos. Window identity is fixed; tray identity follows
//! the system appearance, independently of Emendia's UI theme preference.
use anyhow::{Context, Result};
use gpui_kit::WindowAppearance;
use image::{
    ImageFormat, RgbaImage,
    imageops::{FilterType, resize},
};
use std::sync::OnceLock;

const LIGHT_PNG: &[u8] = include_bytes!("../ressources/app-logo-light.png");
const DARK_PNG: &[u8] = include_bytes!("../ressources/app-logo-dark.png");
const TRAY_SIZE: u32 = 64;
#[cfg(any(target_os = "linux", test))]
const WINDOW_SIZES: [u32; 6] = [16, 32, 48, 64, 128, 256];

pub(crate) fn system_is_dark(appearance: WindowAppearance) -> bool {
    matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

struct Assets {
    light_tray: RgbaImage,
    dark_tray: RgbaImage,
    #[cfg(any(target_os = "linux", test))]
    window_argb: Vec<u32>,
}

static ASSETS: OnceLock<std::result::Result<Assets, String>> = OnceLock::new();

fn assets() -> Result<&'static Assets> {
    ASSETS
        .get_or_init(|| {
            (|| -> Result<Assets> {
                let light = image::load_from_memory_with_format(LIGHT_PNG, ImageFormat::Png)
                    .context("Unable to decode app-logo-light.png")?
                    .into_rgba8();
                let dark = image::load_from_memory_with_format(DARK_PNG, ImageFormat::Png)
                    .context("Unable to decode app-logo-dark.png")?
                    .into_rgba8();
                #[cfg(any(target_os = "linux", test))]
                let window_argb = {
                    let mut values = Vec::new();
                    for size in WINDOW_SIZES {
                        append_argb_icon(
                            &mut values,
                            &resize(&light, size, size, FilterType::Lanczos3),
                        );
                    }
                    values
                };
                Ok(Assets {
                    light_tray: resize(&light, TRAY_SIZE, TRAY_SIZE, FilterType::Lanczos3),
                    dark_tray: resize(&dark, TRAY_SIZE, TRAY_SIZE, FilterType::Lanczos3),
                    #[cfg(any(target_os = "linux", test))]
                    window_argb,
                })
            })()
            .map_err(|error| format!("{error:#}"))
        })
        .as_ref()
        .map_err(|error| anyhow::anyhow!(error.clone()))
}

pub(crate) fn tray_icon(dark_system: bool) -> Result<tray_icon::Icon> {
    let assets = assets()?;
    let image = if dark_system {
        &assets.dark_tray
    } else {
        &assets.light_tray
    };
    Ok(tray_icon::Icon::from_rgba(
        image.as_raw().clone(),
        image.width(),
        image.height(),
    )?)
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn window_icon_argb() -> Result<&'static [u32]> {
    Ok(&assets()?.window_argb)
}

#[cfg(any(target_os = "linux", test))]
fn append_argb_icon(values: &mut Vec<u32>, image: &RgbaImage) {
    values.extend([image.width(), image.height()]);
    values.extend(image.pixels().map(|pixel| {
        // _NET_WM_ICON stores unpremultiplied 0xAARRGGBB CARDINAL values.
        u32::from_be_bytes([pixel[3], pixel[0], pixel[1], pixel[2]])
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x11_argb_encoding_preserves_color_channels_and_alpha() {
        let image =
            RgbaImage::from_raw(2, 1, vec![0x11, 0x22, 0x33, 0x80, 0xab, 0xcd, 0xef, 0]).unwrap();
        let mut values = Vec::new();
        append_argb_icon(&mut values, &image);
        assert_eq!(values, [2, 1, 0x80112233, 0x00abcdef]);
    }

    #[test]
    fn embedded_logos_decode_and_provide_distinct_tray_variants() {
        let assets = assets().unwrap();
        assert_eq!(assets.light_tray.dimensions(), (TRAY_SIZE, TRAY_SIZE));
        assert_eq!(assets.dark_tray.dimensions(), (TRAY_SIZE, TRAY_SIZE));
        assert_ne!(assets.light_tray.as_raw(), assets.dark_tray.as_raw());
        assert!(system_is_dark(WindowAppearance::Dark));
        assert!(system_is_dark(WindowAppearance::VibrantDark));
        assert!(!system_is_dark(WindowAppearance::Light));
        assert!(!system_is_dark(WindowAppearance::VibrantLight));
    }

    #[test]
    fn window_icon_uses_the_fixed_light_logo_at_multiple_sizes() {
        let light = image::load_from_memory_with_format(LIGHT_PNG, ImageFormat::Png)
            .unwrap()
            .into_rgba8();
        let mut values = window_icon_argb().unwrap();
        for size in WINDOW_SIZES {
            assert_eq!(&values[..2], &[size, size]);
            let length = 2 + (size * size) as usize;
            let resized = resize(&light, size, size, FilterType::Lanczos3);
            // Check the base variant, not only the shape of the property.
            for (encoded, rgba) in values[2..length].iter().zip(resized.pixels()) {
                assert_eq!(encoded.to_be_bytes(), [rgba[3], rgba[0], rgba[1], rgba[2]]);
            }
            values = &values[length..];
        }
        assert!(values.is_empty());
    }
}
