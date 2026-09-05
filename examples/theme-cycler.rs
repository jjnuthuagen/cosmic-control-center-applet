//! Demo helper: set the COSMIC dark-theme accent colour (or reset it).
//! Usage: theme-cycler <r> <g> <b>   (0-255 each)   |   theme-cycler reset
use cosmic::cosmic_config::CosmicConfigEntry;
use cosmic::cosmic_theme::{Theme, ThemeBuilder};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let builder_cfg = ThemeBuilder::dark_config().expect("builder config");
    let mut builder = match ThemeBuilder::get_entry(&builder_cfg) {
        Ok(b) => b,
        Err((_, b)) => b,
    };

    if args.first().map(String::as_str) == Some("reset") {
        builder.accent = None;
    } else {
        let px: Vec<f32> = args
            .iter()
            .take(3)
            .map(|s| s.parse::<f32>().expect("0-255 number") / 255.0)
            .collect();
        assert_eq!(px.len(), 3, "usage: theme-cycler <r> <g> <b> | reset");
        builder.accent = Some(cosmic::cosmic_theme::palette::Srgb::new(px[0], px[1], px[2]));
    }

    let theme = builder.build();
    let theme_cfg = Theme::dark_config().expect("theme config");
    if let Err(errs) = theme.write_entry(&theme_cfg) {
        eprintln!("write_entry errors: {errs:?}");
    }
    println!("accent applied");
}
