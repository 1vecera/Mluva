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

pub fn supported(model: &str) -> Vec<&'static Language> {
    LANGUAGES
        .iter()
        .enumerate()
        .filter(|(index, language)| match model {
            "qwen3-1.7b" => *index < 30,
            "parakeet-v3" => [
                "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv",
                "lt", "mt", "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
            ]
            .contains(&language.iso.as_str()),
            _ => language.code != "yue",
        })
        .map(|(_, language)| language)
        .collect()
}
pub fn supports(model: &str, code: &str) -> bool {
    code == "auto"
        || supported(model)
            .iter()
            .any(|language| code == language.code || code == language.iso)
}
pub fn label(code: &str) -> String {
    LANGUAGES
        .iter()
        .find(|language| code == language.code || code == language.iso)
        .map_or_else(
            || {
                if code == "auto" {
                    "◎ Auto-detect".into()
                } else {
                    code.into()
                }
            },
            |language| format!("{} {}", language.flag, language.name),
        )
}
