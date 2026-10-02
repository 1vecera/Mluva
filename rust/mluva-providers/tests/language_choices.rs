//! Every released local-model language and persisted ISO alias, without a GUI.
use mluva_providers::languages;
use serde_json::Value;

#[test]
fn released_local_language_choices_and_aliases() {
    let reference: Value = serde_json::from_str(include_str!(
        "../../mluva-gtk/tests/fixtures/released-provider-pages.json"
    ))
    .unwrap();
    for row in reference["languages"].as_array().unwrap() {
        let model = row["model"].as_str().unwrap();
        let mut supported: Vec<_> = languages::supported(model)
            .iter()
            .map(|language| language.code.clone())
            .collect();
        supported.sort();
        assert_eq!(
            serde_json::to_value(supported).unwrap(),
            row["supported"],
            "{model}: declared languages"
        );
        for choice in row["choices"].as_array().unwrap() {
            let code = choice["code"].as_str().unwrap();
            assert_eq!(
                languages::label(code),
                choice["label"],
                "{model}/{code}: visible language"
            );
            assert_eq!(
                languages::supports(model, code),
                choice["supported"],
                "{model}/{code}: readiness"
            );
        }
    }
}
