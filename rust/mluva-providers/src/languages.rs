use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Clone, Debug, Deserialize)]
pub struct Language {
    pub code: String,
    pub iso: String,
    pub name: String,
    pub flag: String,
}
pub static LANGUAGES: LazyLock<Vec<Language>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../resources/languages.json"))
        .expect("released language catalog")
});

pub fn iso(code: &str) -> &str {
    LANGUAGES
        .iter()
        .find(|language| language.code == code)
        .map_or(code, |language| language.iso.as_str())
}
