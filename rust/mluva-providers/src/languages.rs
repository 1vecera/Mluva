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

/// BCP-47 locale for services that take a region-qualified language, such as Azure Speech. Codes
/// without a listed locale fall back to their two-letter ISO code, which the service may reject.
pub fn locale(code: &str) -> &str {
    match code {
        "eng" => "en-US",
        "ces" => "cs-CZ",
        "spa" => "es-ES",
        "fra" => "fr-FR",
        "deu" => "de-DE",
        "ita" => "it-IT",
        "por" => "pt-BR",
        "nld" => "nl-NL",
        "jpn" => "ja-JP",
        "zho" => "zh-CN",
        "kor" => "ko-KR",
        "pol" => "pl-PL",
        "rus" => "ru-RU",
        "slk" => "sk-SK",
        "ukr" => "uk-UA",
        other => iso(other),
    }
}

/// The three-letter code for a locale or ISO code a service returned, such as `cs-CZ` or `cs`.
pub fn code_for_locale(locale: &str) -> Option<&'static str> {
    let language = locale.split(['-', '_']).next().unwrap_or("");
    LANGUAGES
        .iter()
        .find(|known| known.iso.eq_ignore_ascii_case(language))
        .map(|known| known.code.as_str())
}
