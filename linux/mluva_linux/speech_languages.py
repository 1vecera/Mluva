"""Language names and regional flag hints for the self-contained speech picker."""

# Flags are visual hints, not a claim that a language belongs to one country.
LANGUAGES = (
    ("eng", "en", "English", "🇬🇧"),
    ("zho", "zh", "中文", "🇨🇳"),
    ("yue", "yue", "廣東話", "🇭🇰"),
    ("ara", "ar", "العربية", "🇸🇦"),
    ("deu", "de", "Deutsch", "🇩🇪"),
    ("fra", "fr", "Français", "🇫🇷"),
    ("spa", "es", "Español", "🇪🇸"),
    ("por", "pt", "Português", "🇵🇹"),
    ("ind", "id", "Bahasa Indonesia", "🇮🇩"),
    ("ita", "it", "Italiano", "🇮🇹"),
    ("kor", "ko", "한국어", "🇰🇷"),
    ("rus", "ru", "Русский", "🇷🇺"),
    ("tha", "th", "ไทย", "🇹🇭"),
    ("vie", "vi", "Tiếng Việt", "🇻🇳"),
    ("jpn", "ja", "日本語", "🇯🇵"),
    ("tur", "tr", "Türkçe", "🇹🇷"),
    ("hin", "hi", "हिन्दी", "🇮🇳"),
    ("msa", "ms", "Bahasa Melayu", "🇲🇾"),
    ("nld", "nl", "Nederlands", "🇳🇱"),
    ("swe", "sv", "Svenska", "🇸🇪"),
    ("dan", "da", "Dansk", "🇩🇰"),
    ("fin", "fi", "Suomi", "🇫🇮"),
    ("pol", "pl", "Polski", "🇵🇱"),
    ("ces", "cs", "Čeština", "🇨🇿"),
    ("fil", "fil", "Filipino", "🇵🇭"),
    ("fas", "fa", "فارسی", "🇮🇷"),
    ("ell", "el", "Ελληνικά", "🇬🇷"),
    ("hun", "hu", "Magyar", "🇭🇺"),
    ("mkd", "mk", "Македонски", "🇲🇰"),
    ("ron", "ro", "Română", "🇷🇴"),
    ("slk", "sk", "Slovenčina", "🇸🇰"),
    ("ukr", "uk", "Українська", "🇺🇦"),
    ("bul", "bg", "Български", "🇧🇬"),
    ("hrv", "hr", "Hrvatski", "🇭🇷"),
    ("est", "et", "Eesti", "🇪🇪"),
    ("lav", "lv", "Latviešu", "🇱🇻"),
    ("lit", "lt", "Lietuvių", "🇱🇹"),
    ("mlt", "mt", "Malti", "🇲🇹"),
    ("slv", "sl", "Slovenščina", "🇸🇮"),
)
ISO_CODES = {code: iso for code, iso, _name, _flag in LANGUAGES}
QWEN_LANGUAGES = frozenset(code for code, *_rest in LANGUAGES[:30])
PARAKEET_LANGUAGES = frozenset(
    code
    for code, iso, *_rest in LANGUAGES
    if iso
    in {
        "bg",
        "hr",
        "cs",
        "da",
        "nl",
        "en",
        "et",
        "fi",
        "fr",
        "de",
        "el",
        "hu",
        "it",
        "lv",
        "lt",
        "mt",
        "pl",
        "pt",
        "ro",
        "sk",
        "sl",
        "es",
        "sv",
        "ru",
        "uk",
    }
)


def supported_languages(model):
    """Offer only languages the selected managed model declares."""
    if model == "qwen3-1.7b":
        return QWEN_LANGUAGES
    if model == "parakeet-v3":
        return PARAKEET_LANGUAGES
    return frozenset(code for code, *_rest in LANGUAGES if code != "yue")


def language_label(code):
    """Keep the selected language readable without opening the modal."""
    return next(
        (f"{flag} {name}" for key, _iso, name, flag in LANGUAGES if code in (key, _iso)),
        "◎ Auto-detect" if code == "auto" else code,
    )


def supports_language(model, language):
    """Accept persisted ISO-639-1 and ISO-639-3 forms without silently replacing them."""
    return language == "auto" or any(language in (code, ISO_CODES[code]) for code in supported_languages(model))
