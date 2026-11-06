//! Logo net à toutes les tailles et densités d'écran.
//!
//! Le logo source fait 512 px ; l'afficher à 40 px avec un simple filtre linéaire donne un
//! rendu crénelé. On le réduit donc une fois avec un filtre Lanczos à la taille physique
//! exacte (taille logique × échelle de l'écran), et on le recalcule si l'échelle change.

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use image::{RgbaImage, imageops::FilterType};

pub const LOGO_BYTES: &[u8] = include_bytes!("../../assets/logo.png");

/// Taille (px) de l'icône transmise à la fenêtre.
const WINDOW_ICON_PX: u32 = 128;

fn decode() -> Option<RgbaImage> {
    image::load_from_memory(LOGO_BYTES)
        .ok()
        .map(|i| i.to_rgba8())
}

fn resized(src: &RgbaImage, px: u32) -> RgbaImage {
    image::imageops::resize(src, px, px, FilterType::Lanczos3)
}

fn color_image(img: &RgbaImage) -> ColorImage {
    ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw())
}

/// Icône de fenêtre (utilisée là où le système l'accepte ; sous Wayland, c'est le fichier
/// .desktop et le thème d'icônes qui font foi).
pub fn window_icon() -> Option<egui::IconData> {
    let img = resized(&decode()?, WINDOW_ICON_PX);
    Some(egui::IconData {
        width: img.width(),
        height: img.height(),
        rgba: img.into_raw(),
    })
}

/// Logo prêt à dessiner, mis à l'échelle de l'écran.
pub struct Logo {
    source: Option<RgbaImage>,
    texture: Option<TextureHandle>,
    /// Taille physique (px) de la texture actuelle.
    px: u32,
}

impl Logo {
    pub fn new() -> Self {
        Self {
            source: decode(),
            texture: None,
            px: 0,
        }
    }

    /// Texture adaptée à un affichage de `size_pt` points logiques.
    pub fn texture(&mut self, ctx: &Context, size_pt: f32) -> Option<&TextureHandle> {
        let want = (size_pt * ctx.pixels_per_point()).round().max(8.0) as u32;
        if self.texture.is_none() || self.px != want {
            let src = self.source.as_ref()?;
            let img = if want >= src.width() {
                src.clone()
            } else {
                resized(src, want)
            };
            self.texture =
                Some(ctx.load_texture("logo", color_image(&img), TextureOptions::LINEAR));
            self.px = want;
        }
        self.texture.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_decodes_and_window_icon_is_square() {
        assert!(decode().is_some());
        let i = window_icon().unwrap();
        assert_eq!(i.width, WINDOW_ICON_PX);
        assert_eq!(i.height, WINDOW_ICON_PX);
        assert_eq!(i.rgba.len(), (WINDOW_ICON_PX * WINDOW_ICON_PX * 4) as usize);
    }
}
