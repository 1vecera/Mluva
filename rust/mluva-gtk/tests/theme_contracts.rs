use mluva_gtk::theme::{Palette, build_stylesheet, read_omarchy_palette};
use serde::Deserialize;
use std::fs;

#[derive(Deserialize)]
struct Reference {
    css: Vec<Stylesheet>,
    palettes: Vec<PaletteCase>,
}
#[derive(Deserialize)]
struct Stylesheet {
    tokens: Palette,
    css: String,
}
#[derive(Deserialize)]
struct PaletteCase {
    source: String,
    expected: Option<(Palette, bool)>,
}

#[test]
fn native_semantic_colors_and_validated_omarchy_palettes_match_released_stylesheets() {
    let reference: Reference =
        serde_json::from_str(include_str!("fixtures/theme-cases.json")).unwrap();
    for case in reference.css {
        assert_eq!(build_stylesheet(&case.tokens), case.css);
    }
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("colors.toml");
    for case in reference.palettes {
        fs::write(&path, &case.source).unwrap();
        assert_eq!(
            read_omarchy_palette(&path),
            case.expected,
            "{}",
            case.source
        );
    }
    fs::remove_file(&path).unwrap();
    assert!(read_omarchy_palette(&path).is_none());
}
